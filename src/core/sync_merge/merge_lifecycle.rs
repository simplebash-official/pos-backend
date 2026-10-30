#![forbid(unsafe_code)]

use super::lifecycle_merge::LifecycleMerge;
use super::lifecycle_rank::lifecycle_rank;
use super::meta_keys::META_KEYS;

use serde_json::{Map, Value};

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

/// A quotation's post-creation changes are its lifecycle fields (active -> fulfilled -> cancelled).
const QUOTATION_LIFECYCLE: &[&str] = &["status", "fulfilledByInvoiceKey", "fulfilledAt"];

fn lifecycle_keys(resource: &str) -> Option<&'static [&'static str]> {
    match resource {
        "invoices" | "invoice" => Some(INVOICE_LIFECYCLE),
        "creditNotes" | "credit_notes" | "creditNote" => Some(CREDIT_NOTE_LIFECYCLE),
        "quotations" | "quotation" => Some(QUOTATION_LIFECYCLE),
        _ => None,
    }
}

fn is_invoice(resource: &str) -> bool {
    matches!(resource, "invoices" | "invoice")
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
#[must_use]
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
