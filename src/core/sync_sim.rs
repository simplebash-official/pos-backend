// Deterministic in-memory model of sync v2: replicas that apply changes through
// the real merge engine (`core::sync_merge`), a seeded scenario generator, and a
// hub-and-spoke harness (devices <-> one "cloud" replica). Used by the unit
// tests here and by the convergence harness that drives real backends.
//
// Everything is pure and seeded: the same seed always produces the same run.
// Payload field names mirror the camelCase DTOs the real resources use.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{SecondsFormat, TimeZone, Utc};
use serde_json::{Value, json};

use crate::{
    core::sync_merge::{
        Decision, IncomingChange, META_KEYS, RowMeta, append_only_differs, clamp_updated_at,
        decide, decide_append_only, is_meaningful_conflict, merge_lifecycle,
    },
    domain::sync_v2::{ChangeOp, ChangeRecord, ConflictKind, NewConflict},
    modules::sync::resources::{MergeClass, snake_to_camel, spec},
};

/// SplitMix64: tiny, seedable, no dependencies.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_add(0x9E37_79B9_7F4A_7C15))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (0 when `n == 0`).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }
}

fn iso(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .expect("valid millis")
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn get_str(p: &Value, key: &str) -> String {
    p.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn get_i64(p: &Value, key: &str) -> i64 {
    p.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// One stored row: merge metadata plus the DTO payload.
#[derive(Debug, Clone)]
pub struct SimRow {
    pub meta: RowMeta,
    pub payload: Value,
}

/// Result of applying one change to a replica.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyResult {
    Applied,
    Duplicate,
    KeepLocal,
    Rejected(&'static str),
}

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
    negative_seen: BTreeSet<String>,
    over_refund_seen: BTreeSet<String>,
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

/// One step of a scenario. `sel` fields pick among the rows visible to the
/// acting device at run time.
#[derive(Debug, Clone)]
pub enum Op {
    NewProduct {
        dev: usize,
        price: i64,
    },
    NewCustomer {
        dev: usize,
    },
    PriceEdit {
        dev: usize,
        sel: u64,
        price: i64,
    },
    DeleteProduct {
        dev: usize,
        sel: u64,
    },
    StockAdjust {
        dev: usize,
        sel: u64,
        delta: i64,
    },
    Sale {
        dev: usize,
        sel_product: u64,
        sel_customer: u64,
        qty: i64,
        credit: bool,
        deposit_percent: i64,
    },
    Payment {
        dev: usize,
        sel_invoice: u64,
        amount: i64,
    },
    Return {
        dev: usize,
        sel_invoice: u64,
        qty: i64,
    },
    VoidInvoice {
        dev: usize,
        sel_invoice: u64,
    },
    NoteInvoice {
        dev: usize,
        sel_invoice: u64,
        note: u64,
    },
    Offline {
        dev: usize,
    },
    Online {
        dev: usize,
    },
    Skew {
        dev: usize,
        ms: i64,
        known: bool,
    },
    Sync {
        dev: usize,
    },
}

/// Seeded scenario generator.
pub struct Scenario;

impl Scenario {
    pub fn generate(seed: u64, devices: usize, ops: usize) -> Vec<Op> {
        let mut rng = Rng::new(seed);
        let mut out = Vec::with_capacity(ops + devices * 2);
        // Every device starts with a product and a customer so later ops have targets.
        for dev in 0..devices {
            out.push(Op::NewProduct {
                dev,
                price: 100 + rng.below(900) as i64,
            });
            out.push(Op::NewCustomer { dev });
        }
        for _ in 0..ops {
            let dev = rng.below(devices as u64) as usize;
            let sel = rng.next_u64();
            let op = match rng.below(100) {
                0..=7 => Op::NewProduct {
                    dev,
                    price: 50 + rng.below(950) as i64,
                },
                8..=11 => Op::NewCustomer { dev },
                12..=21 => Op::PriceEdit {
                    dev,
                    sel,
                    price: 50 + rng.below(950) as i64,
                },
                22..=24 => Op::DeleteProduct { dev, sel },
                25..=32 => Op::StockAdjust {
                    dev,
                    sel,
                    delta: rng.below(41) as i64 - 10,
                },
                33..=52 => Op::Sale {
                    dev,
                    sel_product: sel,
                    sel_customer: rng.next_u64(),
                    qty: 1 + rng.below(4) as i64,
                    credit: rng.chance(35),
                    deposit_percent: rng.below(101) as i64,
                },
                53..=59 => Op::Payment {
                    dev,
                    sel_invoice: sel,
                    amount: 10 + rng.below(400) as i64,
                },
                60..=65 => Op::Return {
                    dev,
                    sel_invoice: sel,
                    qty: 1 + rng.below(2) as i64,
                },
                66..=68 => Op::VoidInvoice {
                    dev,
                    sel_invoice: sel,
                },
                69..=72 => Op::NoteInvoice {
                    dev,
                    sel_invoice: sel,
                    note: rng.below(1000),
                },
                73..=79 => Op::Offline { dev },
                80..=86 => Op::Online { dev },
                87..=89 => Op::Skew {
                    dev,
                    ms: rng.below(180_001) as i64 - 90_000,
                    known: rng.chance(50),
                },
                _ => Op::Sync { dev },
            };
            out.push(op);
        }
        out
    }
}

/// One simulated device plus its link state to the hub.
#[derive(Debug, Clone)]
pub struct SimDevice {
    pub replica: SimReplica,
    pub online: bool,
    /// How much of the hub log this device has consumed.
    pub cursor: usize,
    counter: u64,
}

/// Hub-and-spoke run: devices push their outbox to the hub (the cloud
/// replica), the hub logs what it applied, devices pull others' entries.
pub struct Harness {
    pub hub: SimReplica,
    pub devices: Vec<SimDevice>,
    /// Applied changes in hub order, with the pushing device.
    pub log: Vec<(String, ChangeRecord)>,
    pub now_ms: i64,
    rng: Rng,
}

impl Harness {
    pub fn new(devices: usize, seed: u64) -> Self {
        Harness {
            hub: SimReplica::new("cloud"),
            devices: (0..devices)
                .map(|i| SimDevice {
                    replica: SimReplica::new(&format!("dev_{i}")),
                    online: true,
                    cursor: 0,
                    counter: 0,
                })
                .collect(),
            log: Vec::new(),
            now_ms: 1_800_000_000_000,
            rng: Rng::new(seed ^ 0xA5A5_A5A5),
        }
    }

    fn fresh_key(&mut self, dev: usize, prefix: &str) -> String {
        let d = &mut self.devices[dev];
        d.counter += 1;
        format!("{prefix}_{dev}_{}", d.counter)
    }

    /// Runs one op on its device.
    pub fn step(&mut self, op: &Op) {
        self.now_ms += 1 + self.rng.below(400) as i64;
        let now = self.now_ms;
        match *op {
            Op::NewProduct { dev, price } => {
                let key = self.fresh_key(dev, "prod");
                let payload = json!({
                    "id": key, "name": format!("Product {key}"), "priceCents": price,
                    "stockQuantity": 0
                });
                self.devices[dev].replica.local_write(
                    "products",
                    &key,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::NewCustomer { dev } => {
                let key = self.fresh_key(dev, "cus");
                let payload = json!({
                    "id": key, "name": format!("Customer {key}"),
                    "totalPurchasesCents": 0, "outstandingBalanceCents": 0
                });
                self.devices[dev].replica.local_write(
                    "customers",
                    &key,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::PriceEdit { dev, sel, price } => {
                let products = self.devices[dev].replica.visible("products");
                if products.is_empty() {
                    return;
                }
                let (key, mut payload) = products[(sel % products.len() as u64) as usize].clone();
                payload["priceCents"] = json!(price);
                self.devices[dev].replica.local_write(
                    "products",
                    &key,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::DeleteProduct { dev, sel } => {
                let products = self.devices[dev].replica.visible("products");
                if products.len() < 2 {
                    return;
                }
                let (key, _) = products[(sel % products.len() as u64) as usize].clone();
                self.devices[dev].replica.local_write(
                    "products",
                    &key,
                    ChangeOp::Delete,
                    None,
                    now,
                );
            }
            Op::StockAdjust { dev, sel, delta } => {
                let products = self.devices[dev].replica.visible("products");
                if products.is_empty() || delta == 0 {
                    return;
                }
                let (pkey, _) = products[(sel % products.len() as u64) as usize].clone();
                let key = self.fresh_key(dev, "sm");
                let payload = json!({ "id": key, "productId": pkey, "quantityDelta": delta });
                self.devices[dev].replica.local_write(
                    "stockMovements",
                    &key,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::Sale {
                dev,
                sel_product,
                sel_customer,
                qty,
                credit,
                deposit_percent,
            } => {
                let products = self.devices[dev].replica.visible("products");
                let customers = self.devices[dev].replica.visible("customers");
                if products.is_empty() || customers.is_empty() {
                    return;
                }
                let (pkey, product) =
                    products[(sel_product % products.len() as u64) as usize].clone();
                let (ckey, _) = customers[(sel_customer % customers.len() as u64) as usize].clone();
                let unit = get_i64(&product, "priceCents");
                let total = unit * qty;
                let deposit = if credit {
                    total * deposit_percent / 100
                } else {
                    total
                };
                let status = if deposit >= total {
                    "paid"
                } else if deposit > 0 {
                    "partially_paid"
                } else {
                    "pending"
                };
                let ikey = self.fresh_key(dev, "inv");
                let invoice = json!({
                    "id": ikey, "customerKey": ckey, "totalCents": total, "isCredit": credit,
                    "status": status, "notes": null,
                    "items": [ { "productKey": pkey, "unitPriceCents": unit, "quantity": qty, "returnedQuantity": 0 } ],
                    "refundedCents": 0, "creditNoteCount": 0
                });
                let r = &mut self.devices[dev].replica;
                r.local_write("invoices", &ikey, ChangeOp::Upsert, Some(invoice), now);
                if deposit > 0 {
                    let pay = self.fresh_key(dev, "pay");
                    let payload = json!({ "id": pay, "invoiceKey": ikey, "amountCents": deposit });
                    self.devices[dev].replica.local_write(
                        "payments",
                        &pay,
                        ChangeOp::Upsert,
                        Some(payload),
                        now,
                    );
                }
                let sm = self.fresh_key(dev, "sm");
                let payload = json!({ "id": sm, "productId": pkey, "quantityDelta": -qty });
                self.devices[dev].replica.local_write(
                    "stockMovements",
                    &sm,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::Payment {
                dev,
                sel_invoice,
                amount,
            } => {
                let invoices: Vec<_> = self.devices[dev]
                    .replica
                    .visible("invoices")
                    .into_iter()
                    .filter(|(_, p)| {
                        p.get("isCredit").and_then(Value::as_bool).unwrap_or(false)
                            && matches!(get_str(p, "status").as_str(), "pending" | "partially_paid")
                    })
                    .collect();
                if invoices.is_empty() {
                    return;
                }
                let (ikey, _) = invoices[(sel_invoice % invoices.len() as u64) as usize].clone();
                let pay = self.fresh_key(dev, "pay");
                let payload = json!({ "id": pay, "invoiceKey": ikey, "amountCents": amount });
                self.devices[dev].replica.local_write(
                    "payments",
                    &pay,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::Return {
                dev,
                sel_invoice,
                qty,
            } => {
                let invoices: Vec<_> = self.devices[dev]
                    .replica
                    .visible("invoices")
                    .into_iter()
                    .filter(|(_, p)| get_str(p, "status") != "voided")
                    .collect();
                if invoices.is_empty() {
                    return;
                }
                let (ikey, invoice) =
                    invoices[(sel_invoice % invoices.len() as u64) as usize].clone();
                let item = invoice["items"][0].clone();
                let unit = get_i64(&item, "unitPriceCents");
                let pkey = get_str(&item, "productKey");
                let cn = self.fresh_key(dev, "cn");
                let payload = json!({
                    "id": cn, "invoiceKey": ikey, "customerKey": get_str(&invoice, "customerKey"),
                    "refundCashCents": unit * qty, "balanceReductionCents": 0,
                    "status": "resolved",
                    "returnedItems": [ { "productKey": pkey, "quantity": qty } ]
                });
                self.devices[dev].replica.local_write(
                    "creditNotes",
                    &cn,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
                let sm = self.fresh_key(dev, "sm");
                let payload = json!({ "id": sm, "productId": pkey, "quantityDelta": qty });
                self.devices[dev].replica.local_write(
                    "stockMovements",
                    &sm,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::VoidInvoice { dev, sel_invoice } => {
                let invoices: Vec<_> = self.devices[dev]
                    .replica
                    .visible("invoices")
                    .into_iter()
                    .filter(|(_, p)| get_str(p, "status") != "voided")
                    .collect();
                if invoices.is_empty() {
                    return;
                }
                let (ikey, mut payload) =
                    invoices[(sel_invoice % invoices.len() as u64) as usize].clone();
                payload["status"] = json!("voided");
                payload["voidedAt"] = json!(iso(now));
                payload["voidedBy"] = json!(format!("user_{dev}"));
                payload["voidedReason"] = json!("sim");
                self.devices[dev].replica.local_write(
                    "invoices",
                    &ikey,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::NoteInvoice {
                dev,
                sel_invoice,
                note,
            } => {
                let invoices = self.devices[dev].replica.visible("invoices");
                if invoices.is_empty() {
                    return;
                }
                let (ikey, mut payload) =
                    invoices[(sel_invoice % invoices.len() as u64) as usize].clone();
                payload["notes"] = json!(format!("note {note}"));
                self.devices[dev].replica.local_write(
                    "invoices",
                    &ikey,
                    ChangeOp::Upsert,
                    Some(payload),
                    now,
                );
            }
            Op::Offline { dev } => self.devices[dev].online = false,
            Op::Online { dev } => self.devices[dev].online = true,
            Op::Skew { dev, ms, known } => {
                self.devices[dev].replica.clock_skew_ms = ms;
                self.devices[dev].replica.skew_known = known;
            }
            Op::Sync { dev } => self.sync(dev),
        }
    }

    /// Push the outbox to the hub, then pull what other devices sent. No-op offline.
    pub fn sync(&mut self, dev: usize) {
        if !self.devices[dev].online {
            return;
        }
        let device_id = self.devices[dev].replica.device_id.clone();
        let pending = std::mem::take(&mut self.devices[dev].replica.pending);
        if !pending.is_empty() {
            let mut order: Vec<usize> = (0..pending.len()).collect();
            order.sort_by_key(|&i| {
                (
                    spec(&pending[i].resource)
                        .map(|s| s.order)
                        .unwrap_or(u8::MAX),
                    pending[i].key.clone(),
                )
            });
            for i in order {
                let rec = &pending[i];
                if self.hub.apply(rec, Some(self.now_ms)) == ApplyResult::Applied {
                    self.log.push((device_id.clone(), rec.clone()));
                }
            }
            self.hub.derive();
        }
        let cursor = self.devices[dev].cursor;
        let incoming: Vec<ChangeRecord> = self.log[cursor..]
            .iter()
            .filter(|(origin, _)| *origin != device_id)
            .map(|(_, rec)| rec.clone())
            .collect();
        self.devices[dev].cursor = self.log.len();
        if !incoming.is_empty() {
            self.devices[dev].replica.apply_batch(&incoming, None);
        }
    }

    /// Brings every device online and syncs until nothing moves.
    pub fn settle(&mut self) {
        for d in &mut self.devices {
            d.online = true;
        }
        for _ in 0..20 {
            for dev in 0..self.devices.len() {
                self.sync(dev);
            }
            let quiet = self
                .devices
                .iter()
                .all(|d| d.replica.pending.is_empty() && d.cursor == self.log.len());
            if quiet {
                return;
            }
        }
        panic!("sync did not settle");
    }

    pub fn run(&mut self, ops: &[Op]) {
        for op in ops {
            self.step(op);
        }
        self.settle();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fails with just the rows that differ (both sides), not the whole state.
    fn assert_same(left: &BTreeMap<String, Value>, right: &BTreeMap<String, Value>, label: &str) {
        let mut diffs = Vec::new();
        for key in left.keys().chain(right.keys()).collect::<BTreeSet<_>>() {
            if left.get(key) != right.get(key) {
                diffs.push(format!(
                    "{key}\n   left : {:?}\n   right: {:?}",
                    left.get(key),
                    right.get(key)
                ));
            }
        }
        assert!(
            diffs.is_empty(),
            "{label}: {} rows differ:\n{}",
            diffs.len(),
            diffs.iter().take(3).cloned().collect::<Vec<_>>().join("\n")
        );
    }

    fn naive_stock(replica: &SimReplica, product_key: &str) -> i64 {
        replica
            .visible("stockMovements")
            .iter()
            .filter(|(_, p)| get_str(p, "productId") == product_key)
            .map(|(_, p)| get_i64(p, "quantityDelta"))
            .sum()
    }

    fn assert_invariants(replica: &SimReplica, label: &str) {
        for (key, p) in replica.visible("products") {
            assert_eq!(
                get_i64(&p, "stockQuantity"),
                naive_stock(replica, &key),
                "{label}: stock of {key}"
            );
        }
        let invoices = replica.visible("invoices");
        let payments = replica.visible("payments");
        let credit_notes = replica.visible("creditNotes");
        for (key, c) in replica.visible("customers") {
            let mut purchases = 0;
            let mut outstanding = 0;
            for (ikey, inv) in &invoices {
                if get_str(inv, "customerKey") != key || get_str(inv, "status") == "voided" {
                    continue;
                }
                let total = get_i64(inv, "totalCents");
                purchases += total;
                if inv["isCredit"].as_bool().unwrap_or(false) {
                    let paid: i64 = payments
                        .iter()
                        .filter(|(_, p)| get_str(p, "invoiceKey") == *ikey)
                        .map(|(_, p)| get_i64(p, "amountCents"))
                        .sum();
                    outstanding += total - paid;
                }
            }
            for (_, cn) in &credit_notes {
                if get_str(cn, "customerKey") == key && get_str(cn, "status") != "voided" {
                    purchases -= get_i64(cn, "refundCashCents");
                }
            }
            assert_eq!(
                get_i64(&c, "totalPurchasesCents"),
                purchases,
                "{label}: purchases of {key}"
            );
            assert_eq!(
                get_i64(&c, "outstandingBalanceCents"),
                outstanding,
                "{label}: balance of {key}"
            );
        }
        for (ikey, inv) in &invoices {
            let (mut sum, mut count) = (0, 0);
            for (_, cn) in &credit_notes {
                if get_str(cn, "invoiceKey") == *ikey && get_str(cn, "status") != "voided" {
                    sum += get_i64(cn, "refundCashCents") + get_i64(cn, "balanceReductionCents");
                    count += 1;
                }
            }
            assert_eq!(
                get_i64(inv, "refundedCents"),
                sum,
                "{label}: refunded of {ikey}"
            );
            assert_eq!(
                get_i64(inv, "creditNoteCount"),
                count,
                "{label}: count of {ikey}"
            );
        }
    }

    #[test]
    fn rng_is_deterministic_and_shuffle_is_a_permutation() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        assert_eq!(a.next_u64(), b.next_u64());
        let mut v: Vec<u32> = (0..50).collect();
        Rng::new(1).shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(v, sorted);
    }

    #[test]
    fn scenario_generation_is_deterministic() {
        let a = format!("{:?}", Scenario::generate(3, 3, 80));
        let b = format!("{:?}", Scenario::generate(3, 3, 80));
        assert_eq!(a, b);
        assert_ne!(a, format!("{:?}", Scenario::generate(4, 3, 80)));
    }

    #[test]
    fn a_sale_then_a_return_derive_stock_and_balances() {
        let mut h = Harness::new(1, 1);
        h.run(&[
            Op::NewProduct { dev: 0, price: 200 },
            Op::NewCustomer { dev: 0 },
            Op::StockAdjust {
                dev: 0,
                sel: 0,
                delta: 10,
            },
            Op::Sale {
                dev: 0,
                sel_product: 0,
                sel_customer: 0,
                qty: 3,
                credit: true,
                deposit_percent: 50,
            },
            Op::Return {
                dev: 0,
                sel_invoice: 0,
                qty: 1,
            },
        ]);
        let r = &h.devices[0].replica;
        assert_invariants(r, "single device");
        let product = &r.visible("products")[0].1;
        assert_eq!(get_i64(product, "stockQuantity"), 10 - 3 + 1);
        let customer = &r.visible("customers")[0].1;
        // sale 600 total, deposit 300, return refunds 200
        assert_eq!(get_i64(customer, "totalPurchasesCents"), 600 - 200);
        assert_eq!(get_i64(customer, "outstandingBalanceCents"), 300);
        assert_eq!(h.hub.canonical(), r.canonical());
    }

    #[test]
    fn concurrent_price_edits_converge_to_the_later_write() {
        let mut h = Harness::new(2, 2);
        h.step(&Op::NewProduct { dev: 0, price: 100 });
        h.settle();
        h.devices[0].online = false;
        h.devices[1].online = false;
        h.step(&Op::PriceEdit {
            dev: 0,
            sel: 0,
            price: 111,
        });
        h.step(&Op::PriceEdit {
            dev: 1,
            sel: 0,
            price: 222,
        });
        h.settle();
        let a = h.devices[0].replica.canonical();
        assert_eq!(a, h.devices[1].replica.canonical());
        assert_eq!(a, h.hub.canonical());
        let price = get_i64(&h.hub.visible("products")[0].1, "priceCents");
        assert_eq!(price, 222, "device 1 edited last");
        assert!(
            h.hub
                .conflicts
                .iter()
                .any(|c| c.kind == ConflictKind::LwwLoser)
                || h.devices.iter().any(|d| d
                    .replica
                    .conflicts
                    .iter()
                    .any(|c| c.kind == ConflictKind::LwwLoser))
        );
    }

    #[test]
    fn two_offline_sales_of_the_last_unit_converge_and_flag_negative_stock() {
        let mut h = Harness::new(2, 3);
        h.step(&Op::NewProduct { dev: 0, price: 100 });
        h.step(&Op::NewCustomer { dev: 0 });
        h.step(&Op::StockAdjust {
            dev: 0,
            sel: 0,
            delta: 1,
        });
        h.settle();
        h.devices[0].online = false;
        h.devices[1].online = false;
        for dev in 0..2 {
            h.step(&Op::Sale {
                dev,
                sel_product: 0,
                sel_customer: 0,
                qty: 1,
                credit: false,
                deposit_percent: 100,
            });
        }
        h.settle();
        for d in &h.devices {
            assert_invariants(&d.replica, &d.replica.device_id);
            assert_eq!(
                get_i64(&d.replica.visible("products")[0].1, "stockQuantity"),
                -1
            );
        }
        assert_eq!(
            h.devices[0].replica.canonical(),
            h.devices[1].replica.canonical()
        );
        assert!(h.devices.iter().any(|d| {
            d.replica
                .conflicts
                .iter()
                .any(|c| c.kind == ConflictKind::NegativeStock)
        }));
    }

    #[test]
    fn two_devices_returning_the_same_invoice_flag_an_over_refund_and_converge() {
        let mut h = Harness::new(2, 4);
        h.step(&Op::NewProduct { dev: 0, price: 100 });
        h.step(&Op::NewCustomer { dev: 0 });
        h.step(&Op::StockAdjust {
            dev: 0,
            sel: 0,
            delta: 5,
        });
        h.step(&Op::Sale {
            dev: 0,
            sel_product: 0,
            sel_customer: 0,
            qty: 1,
            credit: false,
            deposit_percent: 100,
        });
        h.settle();
        h.devices[0].online = false;
        h.devices[1].online = false;
        for dev in 0..2 {
            h.step(&Op::Return {
                dev,
                sel_invoice: 0,
                qty: 1,
            });
        }
        h.settle();
        assert_eq!(
            h.devices[0].replica.canonical(),
            h.devices[1].replica.canonical()
        );
        let inv = &h.devices[0].replica.visible("invoices")[0].1;
        assert_eq!(get_i64(inv, "refundedCents"), 200);
        assert_eq!(get_i64(inv, "creditNoteCount"), 2);
        assert!(h.devices.iter().any(|d| {
            d.replica
                .conflicts
                .iter()
                .any(|c| c.kind == ConflictKind::OverRefund)
        }));
    }

    #[test]
    fn concurrent_void_and_note_keep_the_void_and_the_latest_note() {
        let mut h = Harness::new(2, 5);
        h.step(&Op::NewProduct { dev: 0, price: 100 });
        h.step(&Op::NewCustomer { dev: 0 });
        h.step(&Op::StockAdjust {
            dev: 0,
            sel: 0,
            delta: 5,
        });
        h.step(&Op::Sale {
            dev: 0,
            sel_product: 0,
            sel_customer: 0,
            qty: 1,
            credit: false,
            deposit_percent: 100,
        });
        h.settle();
        h.devices[0].online = false;
        h.devices[1].online = false;
        h.step(&Op::VoidInvoice {
            dev: 0,
            sel_invoice: 0,
        });
        h.step(&Op::NoteInvoice {
            dev: 1,
            sel_invoice: 0,
            note: 42,
        });
        h.settle();
        let inv = h.hub.visible("invoices")[0].1.clone();
        assert_eq!(get_str(&inv, "status"), "voided");
        assert_eq!(get_str(&inv, "notes"), "note 42");
        assert_eq!(
            h.devices[0].replica.canonical(),
            h.devices[1].replica.canonical()
        );
        assert_eq!(h.devices[0].replica.canonical(), h.hub.canonical());
    }

    #[test]
    fn deleted_product_stays_deleted_on_every_replica() {
        let mut h = Harness::new(2, 6);
        h.run(&[
            Op::NewProduct { dev: 0, price: 100 },
            Op::NewProduct { dev: 1, price: 100 },
        ]);
        h.step(&Op::DeleteProduct { dev: 0, sel: 0 });
        h.settle();
        for d in &h.devices {
            assert_eq!(d.replica.visible("products").len(), 1);
        }
        assert_eq!(
            h.devices[0].replica.canonical(),
            h.devices[1].replica.canonical()
        );
    }

    #[test]
    fn scenarios_converge_across_many_seeds_and_keep_ledger_invariants() {
        for seed in 0..40u64 {
            let devices = 2 + (seed % 3) as usize;
            let ops = Scenario::generate(seed, devices, 250);
            let mut h = Harness::new(devices, seed);
            h.run(&ops);
            let hub = h.hub.canonical();
            assert_invariants(&h.hub, &format!("seed {seed} hub"));
            for d in &h.devices {
                let label = format!("seed {seed} {}", d.replica.device_id);
                assert_same(
                    &d.replica.canonical(),
                    &hub,
                    &format!("{label} diverged from the cloud"),
                );
                assert_invariants(&d.replica, &label);
            }
        }
    }

    #[test]
    fn any_delivery_order_of_the_applied_changes_reaches_the_same_state() {
        for seed in 0..25u64 {
            let devices = 2 + (seed % 3) as usize;
            let mut h = Harness::new(devices, seed);
            h.run(&Scenario::generate(seed, devices, 200));
            let expected = h.hub.canonical();
            let mut records: Vec<ChangeRecord> = h.log.iter().map(|(_, r)| r.clone()).collect();
            let mut rng = Rng::new(seed + 1000);
            for round in 0..6 {
                rng.shuffle(&mut records);
                let mut replica = SimReplica::new("verifier");
                for rec in &records {
                    replica.apply(rec, None);
                }
                replica.derive();
                assert_same(
                    &replica.canonical(),
                    &expected,
                    &format!("seed {seed} round {round}"),
                );
                assert_invariants(&replica, &format!("seed {seed} round {round}"));
            }
        }
    }

    #[test]
    fn applying_the_same_changes_twice_changes_nothing() {
        for seed in 0..15u64 {
            let mut h = Harness::new(3, seed);
            h.run(&Scenario::generate(seed, 3, 200));
            let records: Vec<ChangeRecord> = h.log.iter().map(|(_, r)| r.clone()).collect();
            let mut replica = SimReplica::new("verifier");
            replica.apply_batch(&records, None);
            let once = replica.canonical();
            let second = replica.apply_batch(&records, None);
            assert_eq!(replica.canonical(), once, "seed {seed}");
            assert!(
                second
                    .iter()
                    .all(|r| matches!(r, ApplyResult::Duplicate | ApplyResult::KeepLocal)),
                "seed {seed}: a repeat was applied again"
            );
        }
    }

    #[test]
    fn far_future_timestamps_are_clamped_by_the_receiver_only() {
        let mut hub = SimReplica::new("cloud");
        let now = 1_800_000_000_000;
        let rec = ChangeRecord {
            resource: "products".into(),
            key: "p1".into(),
            op: ChangeOp::Upsert,
            version: 1,
            updated_at: Utc.timestamp_millis_opt(now + 3_600_000).single().unwrap(),
            device_id: "dev_x".into(),
            payload: Some(json!({ "key": "p1", "id": "p1", "priceCents": 5 })),
        };
        assert_eq!(hub.apply(&rec, Some(now)), ApplyResult::Applied);
        let row = &hub.rows[&("products".to_string(), "p1".to_string())];
        assert_eq!(row.meta.updated_at_ms, now);
        let mut device = SimReplica::new("dev_y");
        device.apply(&rec, None);
        let row = &device.rows[&("products".to_string(), "p1".to_string())];
        assert_eq!(row.meta.updated_at_ms, now + 3_600_000);
    }
}
