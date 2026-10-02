// Pure merge engine for sync v2: decides what to do with an incoming change
// given what the receiver already holds. No I/O; both the SQLite device
// applier and the Mongo cloud applier call `decide` so they always agree.
//
// Every rule is a total, order-independent function of (local state, incoming
// change), which is what makes replicas converge no matter the delivery order
// (see the convergence tests in `core::sync_sim`).

#![forbid(unsafe_code)]

use serde_json::{Map, Value};

pub use crate::domain::sync_v2::{ConflictKind, NewConflict};
use crate::{
    domain::sync_v2::{ChangeOp, ChangeRecord},
    modules::sync::resources::{MergeClass, ResourceSpec, snake_to_camel, spec as resource_spec},
};

/// Payload keys that describe the write itself (or are computed on read) and
/// therefore never count as a content difference.
pub const META_KEYS: &[&str] = &[
    "updatedAt",
    "version",
    "deviceId",
    "updatedByDevice",
    "isOverdue",
];

/// Invoice columns that change after creation and merge monotonically.
const INVOICE_LIFECYCLE: &[&str] = &[
    "status",
    "voidedAt",
    "voidedBy",
    "voidedReason",
    "closedAt",
    "closedBy",
];
/// Invoice columns recomputed from ledger rows (never merged from a payload).
const INVOICE_DERIVED: &[&str] = &["refundedCents", "creditNoteCount"];
const CREDIT_NOTE_LIFECYCLE: &[&str] = &["status", "voidedAt", "voidedBy", "voidedReason"];

/// Maximum accepted clock lead of a sender over the receiver.
const MAX_FUTURE_MS: i64 = 5 * 60 * 1000;

/// What the receiver knows about a row (from `sync_row_meta` / Mongo fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowMeta {
    pub updated_at_ms: i64,
    pub device_id: String,
    pub version: i64,
    pub deleted: bool,
}

/// A change with its (possibly clamped) timestamp in epoch milliseconds.
#[derive(Debug, Clone)]
pub struct IncomingChange<'a> {
    pub record: &'a ChangeRecord,
    pub updated_at_ms: i64,
    pub clamped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// The change wins (or the row is new): write it.
    Apply,
    /// Already applied (identical write); nothing to do.
    Duplicate,
    /// The incoming change lost; keep the local row and store the loser.
    KeepLocal { loser: ConflictKind },
    /// Refuse the change with a stable reason string.
    Reject { reason: &'static str },
    /// Lifecycle class: merge only the lifecycle columns (`merge_lifecycle`).
    MergeLifecycle,
}

/// Outcome of merging the lifecycle columns of an invoice / credit note.
#[derive(Debug, Clone, PartialEq)]
pub struct LifecycleMerge {
    /// The local payload with the winning lifecycle columns applied.
    pub merged: Value,
    /// Whether `merged` differs from the local payload.
    pub changed: bool,
    /// `Some("IMMUTABLE_MISMATCH" | "INVALID_PAYLOAD")` when the change is refused.
    pub rejected: Option<&'static str>,
}

/// Decides the fate of `incoming` against the receiver's `local` row meta.
///
/// Rules, in order: (1) malformed / unknown -> reject; (2) no local row ->
/// apply; (3) append-only -> duplicate; (4) lifecycle -> merge lifecycle
/// columns (an identical repeat is a duplicate); (5) LWW on the tuple
/// `(updated_at_ms, device_id)`, deletes included.
pub fn decide(spec: &ResourceSpec, local: Option<&RowMeta>, incoming: &IncomingChange) -> Decision {
    let rec = incoming.record;

    // (1) unknown resource / malformed payload / delete of an undeletable resource
    if resource_spec(&rec.resource).map(|s| s.table) != Some(spec.table) {
        return Decision::Reject {
            reason: "UNKNOWN_RESOURCE",
        };
    }
    match rec.op {
        ChangeOp::Upsert => {
            if !rec.payload.as_ref().is_some_and(Value::is_object) {
                return Decision::Reject {
                    reason: "INVALID_PAYLOAD",
                };
            }
        }
        ChangeOp::Delete => {
            if !spec.soft_delete {
                return Decision::Reject {
                    reason: "INVALID_PAYLOAD",
                };
            }
        }
    }

    // (2) new row
    let Some(local) = local else {
        return Decision::Apply;
    };

    let same_write =
        incoming.updated_at_ms == local.updated_at_ms && rec.device_id == local.device_id;

    match spec.class {
        // (3)
        MergeClass::AppendOnly => decide_append_only(spec, Some(local), incoming, false),
        // (4)
        MergeClass::Lifecycle => {
            if same_write {
                Decision::Duplicate
            } else {
                Decision::MergeLifecycle
            }
        }
        // (5)
        MergeClass::Lww | MergeClass::LwwWithDerived => {
            let incoming_key = (incoming.updated_at_ms, rec.device_id.as_str());
            let local_key = (local.updated_at_ms, local.device_id.as_str());
            match incoming_key.cmp(&local_key) {
                std::cmp::Ordering::Equal => Decision::Duplicate,
                std::cmp::Ordering::Greater => Decision::Apply,
                std::cmp::Ordering::Less => Decision::KeepLocal {
                    loser: ConflictKind::LwwLoser,
                },
            }
        }
    }
}

/// Append-only rule: an existing row is never rewritten. `immutable_differs`
/// is computed by the caller (`append_only_differs`) when the same key arrives
/// from a different write with different content.
pub fn decide_append_only(
    _spec: &ResourceSpec,
    local: Option<&RowMeta>,
    incoming: &IncomingChange,
    immutable_differs: bool,
) -> Decision {
    let Some(local) = local else {
        return Decision::Apply;
    };
    let same_write = incoming.updated_at_ms == local.updated_at_ms
        && incoming.record.device_id == local.device_id;
    if !same_write && immutable_differs {
        Decision::Reject {
            reason: "IMMUTABLE_MISMATCH",
        }
    } else {
        Decision::Duplicate
    }
}

/// Future timestamps beyond `now + 5 min` are clamped to `now`; anything else
/// (including the past) is taken as sent. Returns `(effective_ms, clamped)`.
pub fn clamp_updated_at(incoming_ms: i64, server_now_ms: i64) -> (i64, bool) {
    if incoming_ms > server_now_ms.saturating_add(MAX_FUTURE_MS) {
        (server_now_ms, true)
    } else {
        (incoming_ms, false)
    }
}

/// Position of `status` in the monotonic lifecycle of `resource`, or `None`
/// for an unknown resource/status. Higher rank always wins a merge.
pub fn lifecycle_rank(resource: &str, status: &str) -> Option<u8> {
    match (resource, status) {
        ("invoices" | "invoice", "pending") => Some(0),
        ("invoices" | "invoice", "partially_paid") => Some(1),
        ("invoices" | "invoice", "paid") => Some(2),
        ("invoices" | "invoice", "closed") => Some(3),
        ("invoices" | "invoice", "voided") => Some(4),
        ("creditNotes" | "credit_notes" | "creditNote", "awaiting_resolution") => Some(0),
        ("creditNotes" | "credit_notes" | "creditNote", "resolved") => Some(1),
        ("creditNotes" | "credit_notes" | "creditNote", "voided") => Some(2),
        _ => None,
    }
}

fn lifecycle_keys(resource: &str) -> Option<&'static [&'static str]> {
    match resource {
        "invoices" | "invoice" => Some(INVOICE_LIFECYCLE),
        "creditNotes" | "credit_notes" | "creditNote" => Some(CREDIT_NOTE_LIFECYCLE),
        _ => None,
    }
}

fn is_invoice(resource: &str) -> bool {
    matches!(resource, "invoices" | "invoice")
}

/// `payload` minus write metadata, computed fields and the resource's derived
/// columns: the part two replicas must agree on for the same change.
pub fn strip_for_compare(spec: &ResourceSpec, payload: &Value) -> Value {
    let Value::Object(map) = payload else {
        return payload.clone();
    };
    let derived: Vec<String> = spec.derived.iter().map(|c| snake_to_camel(c)).collect();
    let mut out = Map::new();
    for (k, v) in map {
        if META_KEYS.contains(&k.as_str()) || derived.iter().any(|d| d == k) {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    Value::Object(out)
}

/// True when an append-only row's immutable content differs between two writes.
pub fn append_only_differs(spec: &ResourceSpec, local: &Value, incoming: &Value) -> bool {
    strip_for_compare(spec, local) != strip_for_compare(spec, incoming)
}

/// True when a losing LWW payload differs from the winner in more than
/// timestamps/derived columns, i.e. it is worth showing to the owner.
pub fn is_meaningful_conflict(spec: &ResourceSpec, winning: &Value, losing: &Value) -> bool {
    strip_for_compare(spec, winning) != strip_for_compare(spec, losing)
}

/// Body of an invoice / credit note with lifecycle, derived and meta columns
/// removed (and `items[].returnedQuantity`), i.e. what must never change.
fn immutable_body(resource: &str, payload: &Value) -> Value {
    let Value::Object(map) = payload else {
        return payload.clone();
    };
    let lifecycle = lifecycle_keys(resource).unwrap_or(&[]);
    let mut out = Map::new();
    for (k, v) in map {
        let key = k.as_str();
        if META_KEYS.contains(&key)
            || lifecycle.contains(&key)
            || (is_invoice(resource) && (INVOICE_DERIVED.contains(&key) || key == "notes"))
        {
            continue;
        }
        if key == "items"
            && let Value::Array(items) = v
        {
            let cleaned: Vec<Value> = items
                .iter()
                .map(|item| match item {
                    Value::Object(m) => {
                        let mut m = m.clone();
                        m.remove("returnedQuantity");
                        Value::Object(m)
                    }
                    other => other.clone(),
                })
                .collect();
            out.insert(k.clone(), Value::Array(cleaned));
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    Value::Object(out)
}

/// Sort key for the lifecycle group of a payload; used only to break a tie
/// between two writes reaching the same rank so every replica picks the same.
fn group_tiebreak(resource: &str, payload: &Value) -> Vec<String> {
    lifecycle_keys(resource)
        .unwrap_or(&[])
        .iter()
        .map(|k| {
            payload
                .get(*k)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "null".to_string())
        })
        .collect()
}

/// Merges the lifecycle columns of `incoming` into `local` (both full DTO
/// payloads of the same resource). The body is immutable (first writer wins);
/// `status` takes the higher lifecycle rank with its `voided*`/`closed*`
/// columns travelling along; invoice `notes` is last-writer-wins by the
/// payload's `updatedAt`. The derived `refundedCents`/`creditNoteCount`/
/// `items[].returnedQuantity` are left as local (the caller recomputes them).
pub fn merge_lifecycle(resource: &str, local: &Value, incoming: &Value) -> LifecycleMerge {
    let reject = |reason: &'static str| LifecycleMerge {
        merged: local.clone(),
        changed: false,
        rejected: Some(reason),
    };
    let (Value::Object(local_map), Value::Object(incoming_map)) = (local, incoming) else {
        return reject("INVALID_PAYLOAD");
    };
    let Some(lifecycle) = lifecycle_keys(resource) else {
        return reject("UNKNOWN_RESOURCE");
    };
    if immutable_body(resource, local) != immutable_body(resource, incoming) {
        return reject("IMMUTABLE_MISMATCH");
    }

    let status_of =
        |m: &Map<String, Value>| m.get("status").and_then(Value::as_str).map(str::to_owned);
    let (Some(incoming_status), local_status) = (status_of(incoming_map), status_of(local_map))
    else {
        return reject("INVALID_PAYLOAD");
    };
    let Some(incoming_rank) = lifecycle_rank(resource, &incoming_status) else {
        return reject("INVALID_PAYLOAD");
    };
    let local_rank = local_status
        .as_deref()
        .and_then(|s| lifecycle_rank(resource, s));

    let incoming_wins = match local_rank {
        None => true,
        Some(l) if incoming_rank > l => true,
        Some(l) if incoming_rank < l => false,
        Some(_) => group_tiebreak(resource, incoming) > group_tiebreak(resource, local),
    };

    let mut merged = local_map.clone();
    if incoming_wins {
        for key in lifecycle {
            match incoming_map.get(*key) {
                Some(v) => {
                    merged.insert((*key).to_string(), v.clone());
                }
                None => {
                    merged.remove(*key);
                }
            }
        }
    }

    if is_invoice(resource) {
        let stamp = |m: &Map<String, Value>| {
            m.get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let notes =
            |m: &Map<String, Value>| m.get("notes").map(|v| v.to_string()).unwrap_or_default();
        let incoming_newer =
            (stamp(incoming_map), notes(incoming_map)) > (stamp(local_map), notes(local_map));
        if incoming_newer {
            match incoming_map.get("notes") {
                Some(v) => {
                    merged.insert("notes".to_string(), v.clone());
                }
                None => {
                    merged.remove("notes");
                }
            }
        }
        // The stamp travels with the winning notes: without it a third, older
        // write could later beat them, making the result depend on arrival order.
        let newest = stamp(local_map).max(stamp(incoming_map));
        if !newest.is_empty() {
            merged.insert("updatedAt".to_string(), Value::String(newest));
        }
    }

    let merged = Value::Object(merged);
    let changed = &merged != local;
    LifecycleMerge {
        merged,
        changed,
        rejected: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::sync::resources::spec;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn record(
        resource: &str,
        op: ChangeOp,
        device: &str,
        ms: i64,
        payload: Option<Value>,
    ) -> ChangeRecord {
        ChangeRecord {
            resource: resource.to_string(),
            key: "k1".to_string(),
            op,
            version: 1,
            updated_at: Utc.timestamp_millis_opt(ms).unwrap(),
            device_id: device.to_string(),
            payload,
        }
    }

    fn meta(ms: i64, device: &str) -> RowMeta {
        RowMeta {
            updated_at_ms: ms,
            device_id: device.to_string(),
            version: 1,
            deleted: false,
        }
    }

    fn inc(rec: &ChangeRecord) -> IncomingChange<'_> {
        IncomingChange {
            record: rec,
            updated_at_ms: rec.updated_at.timestamp_millis(),
            clamped: false,
        }
    }

    #[test]
    fn new_row_is_applied_for_every_class() {
        for name in ["products", "stockMovements", "invoices", "categories"] {
            let s = spec(name).unwrap();
            let r = record(
                name,
                ChangeOp::Upsert,
                "dev_a",
                10,
                Some(json!({"key": "k1"})),
            );
            assert_eq!(decide(s, None, &inc(&r)), Decision::Apply, "{name}");
        }
    }

    #[test]
    fn missing_or_non_object_payload_is_rejected() {
        let s = spec("products").unwrap();
        for payload in [None, Some(json!("x")), Some(json!(3))] {
            let r = record("products", ChangeOp::Upsert, "dev_a", 10, payload);
            assert_eq!(
                decide(s, None, &inc(&r)),
                Decision::Reject {
                    reason: "INVALID_PAYLOAD"
                }
            );
        }
    }

    #[test]
    fn unknown_or_mismatched_resource_is_rejected() {
        let s = spec("products").unwrap();
        let r = record("nope", ChangeOp::Upsert, "dev_a", 10, Some(json!({})));
        assert_eq!(
            decide(s, None, &inc(&r)),
            Decision::Reject {
                reason: "UNKNOWN_RESOURCE"
            }
        );
        let r = record("customers", ChangeOp::Upsert, "dev_a", 10, Some(json!({})));
        assert_eq!(
            decide(s, None, &inc(&r)),
            Decision::Reject {
                reason: "UNKNOWN_RESOURCE"
            }
        );
    }

    #[test]
    fn delete_of_an_append_only_or_lifecycle_resource_is_rejected() {
        for name in [
            "stockMovements",
            "payments",
            "invoices",
            "creditNotes",
            "productSerials",
        ] {
            let s = spec(name).unwrap();
            let r = record(name, ChangeOp::Delete, "dev_a", 10, None);
            assert_eq!(
                decide(s, None, &inc(&r)),
                Decision::Reject {
                    reason: "INVALID_PAYLOAD"
                },
                "{name}"
            );
        }
    }

    #[test]
    fn delete_of_unknown_key_records_a_tombstone() {
        let s = spec("products").unwrap();
        let r = record("products", ChangeOp::Delete, "dev_a", 10, None);
        assert_eq!(decide(s, None, &inc(&r)), Decision::Apply);
    }

    #[test]
    fn lww_newer_wins_older_loses_equal_is_duplicate() {
        let s = spec("products").unwrap();
        let local = meta(100, "dev_b");
        let newer = record("products", ChangeOp::Upsert, "dev_a", 101, Some(json!({})));
        let older = record("products", ChangeOp::Upsert, "dev_a", 99, Some(json!({})));
        let same = record("products", ChangeOp::Upsert, "dev_b", 100, Some(json!({})));
        assert_eq!(decide(s, Some(&local), &inc(&newer)), Decision::Apply);
        assert_eq!(
            decide(s, Some(&local), &inc(&older)),
            Decision::KeepLocal {
                loser: ConflictKind::LwwLoser
            }
        );
        assert_eq!(decide(s, Some(&local), &inc(&same)), Decision::Duplicate);
    }

    #[test]
    fn lww_tie_on_time_breaks_by_device_id() {
        let s = spec("customers").unwrap();
        let local = meta(100, "dev_b");
        let higher = record("customers", ChangeOp::Upsert, "dev_c", 100, Some(json!({})));
        let lower = record("customers", ChangeOp::Upsert, "dev_a", 100, Some(json!({})));
        assert_eq!(decide(s, Some(&local), &inc(&higher)), Decision::Apply);
        assert_eq!(
            decide(s, Some(&local), &inc(&lower)),
            Decision::KeepLocal {
                loser: ConflictKind::LwwLoser
            }
        );
    }

    #[test]
    fn delete_versus_edit_newer_wins_both_ways() {
        let s = spec("products").unwrap();
        let live = meta(100, "dev_a");
        let newer_delete = record("products", ChangeOp::Delete, "dev_b", 200, None);
        let older_delete = record("products", ChangeOp::Delete, "dev_b", 50, None);
        assert_eq!(decide(s, Some(&live), &inc(&newer_delete)), Decision::Apply);
        assert_eq!(
            decide(s, Some(&live), &inc(&older_delete)),
            Decision::KeepLocal {
                loser: ConflictKind::LwwLoser
            }
        );
        let tombstone = RowMeta {
            deleted: true,
            ..meta(200, "dev_b")
        };
        let later_edit = record("products", ChangeOp::Upsert, "dev_a", 300, Some(json!({})));
        assert_eq!(
            decide(s, Some(&tombstone), &inc(&later_edit)),
            Decision::Apply
        );
    }

    #[test]
    fn append_only_never_rewrites_an_existing_row() {
        let s = spec("stockMovements").unwrap();
        let local = meta(100, "dev_a");
        let r = record(
            "stockMovements",
            ChangeOp::Upsert,
            "dev_z",
            999,
            Some(json!({})),
        );
        assert_eq!(decide(s, Some(&local), &inc(&r)), Decision::Duplicate);
        assert_eq!(
            decide_append_only(s, Some(&local), &inc(&r), true),
            Decision::Reject {
                reason: "IMMUTABLE_MISMATCH"
            }
        );
        let same = record(
            "stockMovements",
            ChangeOp::Upsert,
            "dev_a",
            100,
            Some(json!({})),
        );
        assert_eq!(
            decide_append_only(s, Some(&local), &inc(&same), true),
            Decision::Duplicate
        );
        assert_eq!(decide_append_only(s, None, &inc(&r), true), Decision::Apply);
    }

    #[test]
    fn lifecycle_class_merges_and_identical_repeat_is_duplicate() {
        let s = spec("invoices").unwrap();
        let local = meta(100, "dev_a");
        let other = record("invoices", ChangeOp::Upsert, "dev_b", 150, Some(json!({})));
        let same = record("invoices", ChangeOp::Upsert, "dev_a", 100, Some(json!({})));
        assert_eq!(
            decide(s, Some(&local), &inc(&other)),
            Decision::MergeLifecycle
        );
        assert_eq!(decide(s, Some(&local), &inc(&same)), Decision::Duplicate);
    }

    #[test]
    fn clamp_only_touches_far_future_timestamps() {
        let now = 1_000_000;
        assert_eq!(clamp_updated_at(now + 299_999, now), (now + 299_999, false));
        assert_eq!(clamp_updated_at(now + 300_000, now), (now + 300_000, false));
        assert_eq!(clamp_updated_at(now + 300_001, now), (now, true));
        assert_eq!(
            clamp_updated_at(now - 86_400_000, now),
            (now - 86_400_000, false)
        );
        assert_eq!(clamp_updated_at(i64::MAX, i64::MAX), (i64::MAX, false));
    }

    #[test]
    fn lifecycle_ranks_are_monotonic() {
        let inv = ["pending", "partially_paid", "paid", "closed", "voided"];
        let ranks: Vec<_> = inv
            .iter()
            .map(|s| lifecycle_rank("invoices", s).unwrap())
            .collect();
        assert_eq!(ranks, [0, 1, 2, 3, 4]);
        let cn = ["awaiting_resolution", "resolved", "voided"];
        let ranks: Vec<_> = cn
            .iter()
            .map(|s| lifecycle_rank("creditNotes", s).unwrap())
            .collect();
        assert_eq!(ranks, [0, 1, 2]);
        assert_eq!(lifecycle_rank("invoices", "nonsense"), None);
        assert_eq!(lifecycle_rank("products", "pending"), None);
    }

    fn invoice(status: &str, extra: Value) -> Value {
        let mut v = json!({
            "key": "inv_1", "id": "aa", "totalCents": 1000, "customerKey": "c1",
            "items": [ { "name": "x", "quantity": 2, "returnedQuantity": 0 } ],
            "status": status, "notes": null, "updatedAt": "2026-01-01T00:00:00.000Z",
            "refundedCents": 0, "creditNoteCount": 0
        });
        for (k, val) in extra.as_object().unwrap() {
            v[k] = val.clone();
        }
        v
    }

    #[test]
    fn higher_lifecycle_rank_wins_regardless_of_order_and_columns_travel() {
        let paid = invoice("paid", json!({}));
        let voided = invoice(
            "voided",
            json!({ "voidedAt": "2026-02-01T00:00:00.000Z", "voidedBy": "u1", "voidedReason": "oops" }),
        );
        let a = merge_lifecycle("invoices", &paid, &voided);
        let b = merge_lifecycle("invoices", &voided, &paid);
        assert_eq!(a.rejected, None);
        assert_eq!(a.merged["status"], "voided");
        assert_eq!(a.merged["voidedBy"], "u1");
        assert!(a.changed);
        // the other order keeps the voided row untouched
        assert_eq!(b.merged["status"], "voided");
        assert_eq!(b.merged["voidedReason"], "oops");
        assert!(!b.changed);
        assert_eq!(a.merged, b.merged);
    }

    #[test]
    fn equal_rank_tie_is_resolved_identically_from_both_sides() {
        let a = invoice(
            "voided",
            json!({ "voidedAt": "2026-02-01T00:00:00.000Z", "voidedBy": "u1" }),
        );
        let b = invoice(
            "voided",
            json!({ "voidedAt": "2026-03-01T00:00:00.000Z", "voidedBy": "u2" }),
        );
        let ab = merge_lifecycle("invoices", &a, &b).merged;
        let ba = merge_lifecycle("invoices", &b, &a).merged;
        assert_eq!(ab, ba);
    }

    #[test]
    fn differing_body_is_rejected_as_immutable_mismatch() {
        let a = invoice("paid", json!({}));
        let b = invoice("paid", json!({ "totalCents": 999 }));
        let m = merge_lifecycle("invoices", &a, &b);
        assert_eq!(m.rejected, Some("IMMUTABLE_MISMATCH"));
        assert!(!m.changed);
        assert_eq!(m.merged, a);
    }

    #[test]
    fn derived_and_meta_columns_do_not_count_as_body_differences() {
        let a = invoice("paid", json!({}));
        let b = invoice(
            "paid",
            json!({
                "refundedCents": 500, "creditNoteCount": 1, "isOverdue": true,
                "updatedAt": "2027-01-01T00:00:00.000Z", "version": 9,
                "items": [ { "name": "x", "quantity": 2, "returnedQuantity": 1 } ]
            }),
        );
        let m = merge_lifecycle("invoices", &a, &b);
        assert_eq!(m.rejected, None);
        // derived columns stay as local; the caller recomputes them
        assert_eq!(m.merged["refundedCents"], 0);
    }

    #[test]
    fn invoice_notes_is_last_writer_wins_and_symmetric() {
        let a = invoice(
            "paid",
            json!({ "notes": "old", "updatedAt": "2026-01-01T00:00:00.000Z" }),
        );
        let b = invoice(
            "paid",
            json!({ "notes": "new", "updatedAt": "2026-01-02T00:00:00.000Z" }),
        );
        assert_eq!(merge_lifecycle("invoices", &a, &b).merged["notes"], "new");
        assert_eq!(merge_lifecycle("invoices", &b, &a).merged["notes"], "new");
    }

    #[test]
    fn invoice_notes_are_order_independent_across_three_writes() {
        let w = |notes: &str, at: &str| invoice("paid", json!({ "notes": notes, "updatedAt": at }));
        let (n1, n2, n3) = (
            w("n1", "2026-01-01T00:00:00.000Z"),
            w("n2", "2026-01-02T00:00:00.000Z"),
            w("n3", "2026-01-03T00:00:00.000Z"),
        );
        let orders = [
            [&n1, &n3, &n2],
            [&n3, &n1, &n2],
            [&n2, &n1, &n3],
            [&n3, &n2, &n1],
        ];
        for order in orders {
            let mut cur = order[0].clone();
            for next in &order[1..] {
                cur = merge_lifecycle("invoices", &cur, next).merged;
            }
            assert_eq!(cur["notes"], "n3", "order {order:?}");
        }
    }

    #[test]
    fn credit_note_status_ranks_and_immutable_body() {
        let cn = |status: &str, extra: Value| {
            let mut v = json!({ "key": "cn_1", "refundCashCents": 100, "status": status });
            for (k, val) in extra.as_object().unwrap() {
                v[k] = val.clone();
            }
            v
        };
        let open = cn("awaiting_resolution", json!({}));
        let resolved = cn("resolved", json!({}));
        let voided = cn("voided", json!({ "voidedBy": "u9" }));
        assert_eq!(
            merge_lifecycle("creditNotes", &open, &resolved).merged["status"],
            "resolved"
        );
        assert_eq!(
            merge_lifecycle("creditNotes", &voided, &resolved).merged["status"],
            "voided"
        );
        let changed_body = cn("resolved", json!({ "refundCashCents": 5 }));
        assert_eq!(
            merge_lifecycle("creditNotes", &open, &changed_body).rejected,
            Some("IMMUTABLE_MISMATCH")
        );
    }

    #[test]
    fn invalid_lifecycle_inputs_are_rejected() {
        let ok = invoice("paid", json!({}));
        assert_eq!(
            merge_lifecycle("invoices", &ok, &json!("x")).rejected,
            Some("INVALID_PAYLOAD")
        );
        let bad_status = invoice("weird", json!({}));
        assert_eq!(
            merge_lifecycle("invoices", &ok, &bad_status).rejected,
            Some("INVALID_PAYLOAD")
        );
        assert_eq!(
            merge_lifecycle("products", &ok, &ok).rejected,
            Some("UNKNOWN_RESOURCE")
        );
    }

    #[test]
    fn no_op_edits_are_not_meaningful_conflicts() {
        let s = spec("products").unwrap();
        let a = json!({ "key": "p1", "priceCents": 100, "stockQuantity": 3, "updatedAt": "x", "version": 2 });
        let b = json!({ "key": "p1", "priceCents": 100, "stockQuantity": 99, "updatedAt": "y", "version": 5 });
        let c = json!({ "key": "p1", "priceCents": 120, "stockQuantity": 3, "updatedAt": "x", "version": 2 });
        assert!(!is_meaningful_conflict(s, &a, &b));
        assert!(is_meaningful_conflict(s, &a, &c));
    }

    #[test]
    fn append_only_difference_detection_ignores_meta() {
        let s = spec("stockMovements").unwrap();
        let a = json!({ "key": "sm_1", "quantityDelta": -1, "updatedAt": "x" });
        let b = json!({ "key": "sm_1", "quantityDelta": -1, "updatedAt": "y", "version": 3 });
        let c = json!({ "key": "sm_1", "quantityDelta": -2 });
        assert!(!append_only_differs(s, &a, &b));
        assert!(append_only_differs(s, &a, &c));
    }
}
