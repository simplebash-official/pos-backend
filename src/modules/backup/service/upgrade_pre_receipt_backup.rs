use serde_json::{Value, json};
use std::collections::HashMap;

/// A backup taken before supplier receipts carries purchases with no `receipt_key`, which
/// today's schema requires. Rewrites such a backup, in memory, exactly the way
/// `clients::sqlite::migrate_purchases_to_receipts` rewrites a live database: purchases of one
/// supplier, invoice number and day share a receipt keyed by the smallest purchase key; a
/// purchase without an invoice number is a receipt of its own. A backup whose purchases all
/// have a receipt is left untouched.
pub(super) fn upgrade_pre_receipt_backup(tables: &mut HashMap<String, Vec<Value>>) {
    let Some(purchases) = tables.get_mut("purchases") else {
        return;
    };
    if purchases.iter().all(|p| p.get("receipt_key").is_some()) {
        return;
    }

    let text = |row: &Value, field: &str| {
        row.get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let group_of = |row: &Value| {
        let key = text(row, "key").unwrap_or_default();
        (
            text(row, "supplier_key").unwrap_or_default(),
            text(row, "reference_no").unwrap_or_else(|| key.clone()),
            text(row, "date").unwrap_or_default(),
        )
    };

    let mut first_key: HashMap<(String, String, String), String> = HashMap::new();
    for purchase in purchases.iter().filter(|p| p.get("receipt_key").is_none()) {
        let key = text(purchase, "key").unwrap_or_default();
        first_key
            .entry(group_of(purchase))
            .and_modify(|first| {
                if key < *first {
                    *first = key.clone();
                }
            })
            .or_insert(key);
    }

    let mut receipts: HashMap<String, Value> = HashMap::new();
    for purchase in purchases.iter_mut() {
        if purchase.get("receipt_key").is_some() {
            continue;
        }
        let first = first_key[&group_of(purchase)].clone();
        let receipt_key = format!("prc_leg_{first}");
        let created_at = text(purchase, "created_at").unwrap_or_default();
        receipts.entry(receipt_key.clone()).or_insert_with(|| {
            let tail: String = first
                .chars()
                .rev()
                .take(10)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let hex: String = first.bytes().map(|b| format!("{b:02X}")).collect();
            json!({
                "key": receipt_key,
                "id": hex[hex.len().saturating_sub(24)..],
                "receipt_number": format!("PUR-L{tail}"),
                "supplier_key": purchase["supplier_key"],
                "reference_no": text(purchase, "reference_no"),
                "date": purchase["date"],
                "notes": text(purchase, "notes"),
                "version": 1,
                "created_at": created_at,
                "updated_at": created_at,
            })
        });
        if let Some(object) = purchase.as_object_mut() {
            object.insert("receipt_key".into(), json!(receipt_key));
            object.remove("reference_no");
            object.remove("notes");
        }
    }

    let rows = tables.entry("purchase_receipts".to_string()).or_default();
    let mut migrated: Vec<Value> = receipts.into_values().collect();
    migrated.sort_by(|a, b| a["key"].as_str().cmp(&b["key"].as_str()));
    rows.extend(migrated);
}
