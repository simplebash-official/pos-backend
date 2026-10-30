use crate::core::constants::prefixes::opening_batch_key;
use crate::modules::inventory::service::stock_batch::backfill_line_allocations;
use serde_json::{Value, json};
use std::collections::HashMap;

/// A backup taken before stock batches existed carries each product's price
/// on the product row and no `stock_batches` table, while today's schema
/// needs a batch behind every unit (and a batch key on serials, a price on
/// purchases). Rewrites such a backup, in memory, exactly the way
/// `clients::sqlite::migrate_to_stock_batches` rewrites a live database: one
/// opening batch per product holding its stock at its last cost and price,
/// movements/serials pointed at it, purchases priced, sale lines allocated.
/// A backup that already has batches is left untouched.
pub(super) fn upgrade_pre_batch_backup(tables: &mut HashMap<String, Vec<Value>>) {
    let products = tables.get("products").cloned().unwrap_or_default();
    let pre_batch = products.iter().any(|p| p.get("cost_price_cents").is_some());
    if !pre_batch
        || tables
            .get("stock_batches")
            .is_some_and(|rows| !rows.is_empty())
    {
        return;
    }

    let int = |row: &Value, field: &str| row.get(field).and_then(Value::as_i64).unwrap_or(0);
    let mut key_by_id: HashMap<String, String> = HashMap::new();
    let mut price_by_key: HashMap<String, i64> = HashMap::new();
    let mut cost_by_key: HashMap<String, i64> = HashMap::new();
    let mut batches = Vec::with_capacity(products.len());
    for product in &products {
        let (Some(key), Some(id)) = (
            product.get("key").and_then(Value::as_str),
            product.get("id").and_then(Value::as_str),
        ) else {
            continue;
        };
        key_by_id.insert(id.to_string(), key.to_string());
        price_by_key.insert(key.to_string(), int(product, "selling_price_cents"));
        cost_by_key.insert(key.to_string(), int(product, "cost_price_cents"));
        let stock = int(product, "stock_quantity");
        let created_at = product.get("created_at").cloned().unwrap_or(Value::Null);
        batches.push(json!({
            "key": opening_batch_key(key),
            "id": id,
            "product_key": key,
            "source": "opening",
            "purchase_key": null,
            "supplier_key": null,
            "received_quantity": stock,
            "remaining_quantity": stock,
            "unit_cost_cents": int(product, "cost_price_cents"),
            "selling_price_cents": int(product, "selling_price_cents"),
            "received_at": created_at,
            "version": 1,
            "created_at": created_at,
            "updated_at": created_at,
            "deleted_at": product.get("deleted_at").cloned().unwrap_or(Value::Null),
            "updated_by_device": null,
        }));
    }
    tables.insert("stock_batches".to_string(), batches);

    for movement in tables.get_mut("stock_movements").into_iter().flatten() {
        let has_batch = movement.get("batch_key").is_some_and(|v| !v.is_null());
        if has_batch || int(movement, "quantity_delta") == 0 {
            continue;
        }
        if let Some(key) = movement
            .get("product_id")
            .and_then(Value::as_str)
            .and_then(|id| key_by_id.get(id))
        {
            movement["batch_key"] = json!(opening_batch_key(key));
        }
    }
    for serial in tables.get_mut("product_serials").into_iter().flatten() {
        if let Some(key) = serial
            .get("product_key")
            .and_then(Value::as_str)
            .map(str::to_string)
        {
            serial["batch_key"] = json!(opening_batch_key(&key));
        }
    }
    for purchase in tables.get_mut("purchases").into_iter().flatten() {
        if let Some(price) = purchase
            .get("product_key")
            .and_then(Value::as_str)
            .and_then(|key| price_by_key.get(key))
        {
            purchase["selling_price_cents"] = json!(price);
        }
    }

    let cost_of = |key: &str| cost_by_key.get(key).copied();
    for (table, columns) in [
        ("invoices", &["items"][..]),
        ("credit_notes", &["returned_items", "exchange_items"][..]),
    ] {
        for row in tables.get_mut(table).into_iter().flatten() {
            for column in columns {
                let Some(mut lines) = row
                    .get(*column)
                    .and_then(Value::as_str)
                    .and_then(|text| serde_json::from_str::<Value>(text).ok())
                else {
                    continue;
                };
                if backfill_line_allocations(&mut lines, cost_of) {
                    row[*column] = Value::String(lines.to_string());
                }
            }
        }
    }
}
