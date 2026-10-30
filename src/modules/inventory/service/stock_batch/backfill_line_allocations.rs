/// One-time migration to stock batches, for a list of stored sale lines
/// (an invoice's `items`, a credit note's `returnedItems`/`exchangeItems`,
/// camelCase JSON): gives every product line that has none a single
/// allocation from the product's opening batch
/// (`prefixes::opening_batch_key`), at the line's recorded unit cost or, when
/// the line never recorded one, the product's last cost (`cost_of`). Returns
/// whether anything changed. Used by the SQLite migration
/// (`clients::sqlite::migrate_to_stock_batches`) and the Mongo one
/// (`bin/migrate_stock_batches.rs`) so both rewrite history identically.
pub fn backfill_line_allocations(
    items: &mut serde_json::Value,
    cost_of: impl Fn(&str) -> Option<i64>,
) -> bool {
    let Some(lines) = items.as_array_mut() else {
        return false;
    };
    let mut changed = false;
    for line in lines {
        let Some(product_key) = line.get("productKey").and_then(|v| v.as_str()) else {
            continue;
        };
        let has_allocations = line
            .get("batchAllocations")
            .and_then(|v| v.as_array())
            .is_some_and(|a| !a.is_empty());
        let quantity = line.get("quantity").and_then(|v| v.as_i64()).unwrap_or(0);
        if has_allocations || quantity <= 0 {
            continue;
        }
        let Some(unit_cost_cents) = line
            .get("unitCostCents")
            .and_then(|v| v.as_i64())
            .or_else(|| cost_of(product_key))
        else {
            continue;
        };
        let batch_key = crate::core::constants::prefixes::opening_batch_key(product_key);
        line["batchAllocations"] = serde_json::json!([{
            "batchKey": batch_key,
            "quantity": quantity,
            "unitCostCents": unit_cost_cents,
        }]);
        changed = true;
    }
    changed
}
