#![forbid(unsafe_code)]
use super::append_only_differs::append_only_differs;
use super::clamp_updated_at::clamp_updated_at;
use super::decide::decide;
use super::decide_append_only::decide_append_only;
use super::decision::Decision;
use super::incoming_change::IncomingChange;
use super::is_meaningful_conflict::is_meaningful_conflict;
use super::lifecycle_rank::lifecycle_rank;
use super::merge_lifecycle::merge_lifecycle;
use super::row_meta::RowMeta;
use crate::domain::sync_v2::{ChangeOp, ChangeRecord};

use serde_json::Value;

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
        "quotations",
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

fn quotation(status: &str, total_cents: i64) -> Value {
    json!({
        "key": "quo_1", "quotationNumber": "QUO-0001", "status": status,
        "totalCents": total_cents, "items": [{ "name": "Screen", "quantity": 1 }],
        "shopProfileSnapshot": { "tradingName": "TechFix" },
        "updatedAt": "2026-10-13T09:00:00Z", "version": 1
    })
}

#[test]
fn quotation_lifecycle_only_moves_forward() {
    assert_eq!(lifecycle_rank("quotations", "active"), Some(0));
    assert_eq!(lifecycle_rank("quotations", "fulfilled"), Some(1));
    assert_eq!(lifecycle_rank("quotations", "cancelled"), Some(2));
    // `expired` is derived from the date, never a stored status.
    assert_eq!(lifecycle_rank("quotations", "expired"), None);
}

#[test]
fn a_cancelled_quotation_wins_in_every_delivery_order_and_is_never_undone() {
    let active = quotation("active", 1000);
    let cancelled = quotation("cancelled", 1000);

    // active then cancel, cancel then a late (stale) active: both end cancelled.
    let forward = merge_lifecycle("quotations", &active, &cancelled);
    assert_eq!(forward.merged["status"], "cancelled");
    assert!(forward.changed && forward.rejected.is_none());

    let stale = merge_lifecycle("quotations", &cancelled, &active);
    assert_eq!(stale.merged["status"], "cancelled");
    assert!(!stale.changed, "a stale active write must change nothing");
}

#[test]
fn cancelling_twice_is_a_no_op() {
    let cancelled = quotation("cancelled", 1000);
    let again = merge_lifecycle("quotations", &cancelled, &cancelled.clone());
    assert!(!again.changed);
    assert!(again.rejected.is_none());
}

#[test]
fn a_quotation_body_is_immutable_and_unknown_statuses_are_rejected() {
    let active = quotation("active", 1000);
    // Same key, different content: a replica must never silently adopt it.
    assert_eq!(
        merge_lifecycle("quotations", &active, &quotation("cancelled", 999)).rejected,
        Some("IMMUTABLE_MISMATCH")
    );
    assert_eq!(
        merge_lifecycle("quotations", &active, &quotation("expired", 1000)).rejected,
        Some("INVALID_PAYLOAD")
    );
    // Write metadata never counts as a content difference.
    let mut later = quotation("cancelled", 1000);
    later["updatedAt"] = json!("2026-10-14T09:00:00Z");
    later["version"] = json!(2);
    later["updatedByDevice"] = json!("dev_b");
    assert!(
        merge_lifecycle("quotations", &active, &later)
            .rejected
            .is_none()
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
