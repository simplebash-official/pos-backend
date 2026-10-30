use super::core_sync_sim::ApplyResult;
use super::core_sync_sim::SimRow;
use super::core_sync_sim::get_i64;
use super::core_sync_sim::get_str;
use super::core_sync_sim::iso;
use crate::core::sync_merge::{
    Decision, IncomingChange, META_KEYS, RowMeta, append_only_differs, clamp_updated_at, decide,
    decide_append_only, is_meaningful_conflict, merge_lifecycle,
};
use crate::domain::sync_v2::{ChangeOp, ChangeRecord, ConflictKind, NewConflict};
use crate::modules::sync::resources::{MergeClass, snake_to_camel, spec};
use chrono::{TimeZone, Utc};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A device (or the cloud): rows keyed by `(resource, key)`, an outbox, and the
/// conflicts it has raised.
#[derive(Debug, Clone)]
pub struct SimReplica {
    pub device_id: String,
    pub rows: BTreeMap<(String, String), SimRow>,
    pub pending: Vec<ChangeRecord>,
    pub conflicts: Vec<NewConflict>,
    /// How far this device's clock is off the true time.
    pub clock_skew_ms: i64,
    /// Whether the device has measured (and so compensates for) its skew.
    pub skew_known: bool,
    pub(super) negative_seen: BTreeSet<String>,
    pub(super) over_refund_seen: BTreeSet<String>,
}

impl SimReplica {
    pub fn new(device_id: &str) -> Self {
        SimReplica {
            device_id: device_id.to_string(),
            rows: BTreeMap::new(),
            pending: Vec::new(),
            conflicts: Vec::new(),
            clock_skew_ms: 0,
            skew_known: false,
            negative_seen: BTreeSet::new(),
            over_refund_seen: BTreeSet::new(),
        }
    }

    /// Applies one incoming change through the merge engine. `server_now_ms`
    /// enables the receiver-side future-timestamp clamp (used by the cloud).
    pub fn apply(&mut self, rec: &ChangeRecord, server_now_ms: Option<i64>) -> ApplyResult {
        let Some(sp) = spec(&rec.resource) else {
            return ApplyResult::Rejected("UNKNOWN_RESOURCE");
        };
        let raw_ms = rec.updated_at.timestamp_millis();
        let (ms, clamped) = match server_now_ms {
            Some(now) => clamp_updated_at(raw_ms, now),
            None => (raw_ms, false),
        };
        let key = (sp.name.to_string(), rec.key.clone());
        let local = self.rows.get(&key).cloned();
        let local_meta = local.as_ref().map(|r| r.meta.clone());
        let incoming = IncomingChange {
            record: rec,
            updated_at_ms: ms,
            clamped,
        };

        let mut decision = decide(sp, local_meta.as_ref(), &incoming);
        if sp.class == MergeClass::AppendOnly
            && let (Decision::Duplicate, Some(row), Some(payload)) =
                (&decision, &local, rec.payload.as_ref())
        {
            let differs = append_only_differs(sp, &row.payload, payload);
            decision = decide_append_only(sp, local_meta.as_ref(), &incoming, differs);
        }

        match decision {
            Decision::Reject { reason } => ApplyResult::Rejected(reason),
            Decision::Duplicate => ApplyResult::Duplicate,
            Decision::KeepLocal { loser } => {
                if let (Some(row), Some(payload)) = (&local, rec.payload.as_ref())
                    && is_meaningful_conflict(sp, &row.payload, payload)
                {
                    self.conflicts.push(NewConflict {
                        kind: loser,
                        resource: sp.name.to_string(),
                        entity_key: rec.key.clone(),
                        detail: json!({ "losingPayload": payload, "winningDeviceId": row.meta.device_id }),
                    });
                }
                ApplyResult::KeepLocal
            }
            Decision::Apply => {
                let meta = RowMeta {
                    updated_at_ms: ms,
                    device_id: rec.device_id.clone(),
                    version: rec.version,
                    deleted: rec.op == ChangeOp::Delete,
                };
                let old_payload = local.map(|r| r.payload);
                let payload = match rec.op {
                    ChangeOp::Upsert => {
                        let mut p = rec.payload.clone().unwrap_or(Value::Null);
                        if sp.class == MergeClass::LwwWithDerived
                            && let Value::Object(map) = &mut p
                        {
                            for column in sp.derived {
                                let field = snake_to_camel(column);
                                map.remove(&field);
                                if let Some(old) = old_payload.as_ref().and_then(|o| o.get(&field))
                                {
                                    map.insert(field, old.clone());
                                }
                            }
                        }
                        p
                    }
                    ChangeOp::Delete => old_payload.unwrap_or_else(|| json!({ "key": rec.key })),
                };
                self.rows.insert(key, SimRow { meta, payload });
                ApplyResult::Applied
            }
            Decision::MergeLifecycle => {
                let Some(row) = self.rows.get_mut(&key) else {
                    return ApplyResult::Rejected("INVALID_PAYLOAD");
                };
                let incoming_payload = rec.payload.clone().unwrap_or(Value::Null);
                let merged = merge_lifecycle(sp.name, &row.payload, &incoming_payload);
                if let Some(reason) = merged.rejected {
                    return ApplyResult::Rejected(reason);
                }
                let mut changed = merged.changed;
                row.payload = merged.merged;
                if (ms, rec.device_id.as_str())
                    > (row.meta.updated_at_ms, row.meta.device_id.as_str())
                {
                    row.meta.updated_at_ms = ms;
                    row.meta.device_id = rec.device_id.clone();
                    changed = true;
                }
                if rec.version > row.meta.version {
                    row.meta.version = rec.version;
                    changed = true;
                }
                if changed {
                    ApplyResult::Applied
                } else {
                    ApplyResult::Duplicate
                }
            }
        }
    }

    /// Applies a batch in resource order (parents first), then recomputes the
    /// derived fields, exactly like the real applier does per batch.
    pub fn apply_batch(
        &mut self,
        changes: &[ChangeRecord],
        server_now_ms: Option<i64>,
    ) -> Vec<ApplyResult> {
        let mut order: Vec<usize> = (0..changes.len()).collect();
        order.sort_by_key(|&i| {
            let c = &changes[i];
            (
                spec(&c.resource).map(|s| s.order).unwrap_or(u8::MAX),
                c.key.clone(),
            )
        });
        let mut results = vec![ApplyResult::Duplicate; changes.len()];
        for i in order {
            results[i] = self.apply(&changes[i], server_now_ms);
        }
        self.derive();
        results
    }

    /// A write made on this device: stamped with its (corrected) clock, made
    /// monotonic per row, applied locally and queued for push.
    pub fn local_write(
        &mut self,
        resource: &str,
        key: &str,
        op: ChangeOp,
        payload: Option<Value>,
        true_now_ms: i64,
    ) -> ChangeRecord {
        let sp = spec(resource).expect("known resource");
        let device_time = true_now_ms + self.clock_skew_ms;
        let corrected = if self.skew_known {
            device_time - self.clock_skew_ms
        } else {
            device_time
        };
        let prev = self.rows.get(&(sp.name.to_string(), key.to_string()));
        let ms = match prev {
            Some(row) => corrected.max(row.meta.updated_at_ms + 1),
            None => corrected,
        };
        let version = prev.map(|r| r.meta.version + 1).unwrap_or(1);
        let payload = payload.map(|mut p| {
            if let Value::Object(map) = &mut p {
                map.insert("key".into(), json!(key));
                map.insert("updatedAt".into(), json!(iso(ms)));
                map.insert("version".into(), json!(version));
            }
            p
        });
        let rec = ChangeRecord {
            resource: sp.name.to_string(),
            key: key.to_string(),
            op,
            version,
            updated_at: Utc.timestamp_millis_opt(ms).single().expect("valid millis"),
            device_id: self.device_id.clone(),
            payload,
        };
        self.apply(&rec, None);
        self.pending
            .retain(|p| !(p.resource == rec.resource && p.key == rec.key));
        self.pending.push(rec.clone());
        self.derive();
        rec
    }

    /// Live (not deleted) rows of a resource, sorted by key.
    pub fn visible(&self, resource: &str) -> Vec<(String, Value)> {
        self.rows
            .iter()
            .filter(|((res, _), row)| res == resource && !row.meta.deleted)
            .map(|((_, key), row)| (key.clone(), row.payload.clone()))
            .collect()
    }

    /// Recomputes every derived field from the ledger rows (spec section 1.3).
    pub fn derive(&mut self) {
        let mut movement: HashMap<String, i64> = HashMap::new();
        let mut paid: HashMap<String, i64> = HashMap::new();
        let mut refunded: HashMap<String, (i64, i64)> = HashMap::new();
        let mut customer_refund: HashMap<String, i64> = HashMap::new();
        for ((res, _), row) in &self.rows {
            if row.meta.deleted {
                continue;
            }
            let p = &row.payload;
            match res.as_str() {
                "stockMovements" => {
                    *movement.entry(get_str(p, "productId")).or_default() +=
                        get_i64(p, "quantityDelta");
                }
                "payments" => {
                    *paid.entry(get_str(p, "invoiceKey")).or_default() += get_i64(p, "amountCents");
                }
                "creditNotes" if get_str(p, "status") != "voided" => {
                    let entry = refunded.entry(get_str(p, "invoiceKey")).or_default();
                    entry.0 += get_i64(p, "refundCashCents") + get_i64(p, "balanceReductionCents");
                    entry.1 += 1;
                    *customer_refund
                        .entry(get_str(p, "customerKey"))
                        .or_default() += get_i64(p, "refundCashCents");
                }
                _ => {}
            }
        }

        // Invoices: refund totals and (for credit sales) status from payments.
        let mut purchases: HashMap<String, i64> = HashMap::new();
        let mut outstanding: HashMap<String, i64> = HashMap::new();
        let mut new_conflicts = Vec::new();
        for ((res, key), row) in self.rows.iter_mut() {
            if res != "invoices" || row.meta.deleted {
                continue;
            }
            let (refund_sum, refund_count) = refunded.get(key).copied().unwrap_or((0, 0));
            let total = get_i64(&row.payload, "totalCents");
            let is_credit = row
                .payload
                .get("isCredit")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let status = get_str(&row.payload, "status");
            let paid_sum = paid.get(key).copied().unwrap_or(0);
            let new_status =
                if is_credit && matches!(status.as_str(), "pending" | "partially_paid" | "paid") {
                    if paid_sum >= total {
                        "paid"
                    } else if paid_sum > 0 {
                        "partially_paid"
                    } else {
                        "pending"
                    }
                    .to_string()
                } else {
                    status
                };
            if let Value::Object(map) = &mut row.payload {
                map.insert("refundedCents".into(), json!(refund_sum));
                map.insert("creditNoteCount".into(), json!(refund_count));
                map.insert("status".into(), json!(new_status));
            }
            if refund_sum > total && self.over_refund_seen.insert(key.clone()) {
                new_conflicts.push(NewConflict {
                    kind: ConflictKind::OverRefund,
                    resource: "invoices".into(),
                    entity_key: key.clone(),
                    detail: json!({ "refundedCents": refund_sum, "totalCents": total }),
                });
            }
            let customer = get_str(&row.payload, "customerKey");
            if new_status != "voided" && !customer.is_empty() {
                *purchases.entry(customer.clone()).or_default() += total;
                if is_credit {
                    *outstanding.entry(customer).or_default() += total - paid_sum;
                }
            }
        }

        for ((res, key), row) in self.rows.iter_mut() {
            if row.meta.deleted {
                continue;
            }
            match res.as_str() {
                "products" => {
                    let id = row
                        .payload
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or(key)
                        .to_string();
                    let stock = movement.get(&id).copied().unwrap_or(0);
                    if let Value::Object(map) = &mut row.payload {
                        map.insert("stockQuantity".into(), json!(stock));
                    }
                    if stock < 0 {
                        if self.negative_seen.insert(key.clone()) {
                            new_conflicts.push(NewConflict {
                                kind: ConflictKind::NegativeStock,
                                resource: "products".into(),
                                entity_key: key.clone(),
                                detail: json!({ "quantity": stock }),
                            });
                        }
                    } else {
                        self.negative_seen.remove(key);
                    }
                }
                "customers" => {
                    let total = purchases.get(key).copied().unwrap_or(0)
                        - customer_refund.get(key).copied().unwrap_or(0);
                    if let Value::Object(map) = &mut row.payload {
                        map.insert("totalPurchasesCents".into(), json!(total));
                        map.insert(
                            "outstandingBalanceCents".into(),
                            json!(outstanding.get(key).copied().unwrap_or(0)),
                        );
                    }
                }
                _ => {}
            }
        }
        self.conflicts.extend(new_conflicts);
    }

    /// Order-independent view used to compare replicas: every row with its merge
    /// tuple and payload, write metadata stripped.
    pub fn canonical(&self) -> BTreeMap<String, Value> {
        self.rows
            .iter()
            .map(|((res, key), row)| {
                // A tombstone has no content: what a deleted row still holds
                // depends on the order changes arrived in and is meaningless.
                let mut payload = if row.meta.deleted {
                    Value::Null
                } else {
                    row.payload.clone()
                };
                if let Value::Object(map) = &mut payload {
                    for k in META_KEYS {
                        map.remove(*k);
                    }
                }
                (
                    format!("{res}/{key}"),
                    json!({
                        "deleted": row.meta.deleted,
                        "meta": [row.meta.updated_at_ms, row.meta.device_id],
                        "payload": payload,
                    }),
                )
            })
            .collect()
    }
}
