use serde_json::{Value, json};
use std::collections::HashMap;

/// A backup taken before lot prices and extra charges prices each purchase line per unit
/// (`unit_cost_cents`); today's schema holds the line's total (`line_total_cents`) and its share
/// of extra charges. Rewrites such lines, in memory, exactly the way
/// `clients::sqlite::migrate_schema` rewrites a live database: the total is quantity x the old
/// unit cost and there is no share. A backup whose lines already have a total is left untouched.
pub(super) fn upgrade_pre_line_total_backup(tables: &mut HashMap<String, Vec<Value>>) {
    let Some(purchases) = tables.get_mut("purchases") else {
        return;
    };
    for purchase in purchases.iter_mut() {
        let Some(row) = purchase.as_object_mut() else {
            continue;
        };
        if row.contains_key("line_total_cents") {
            continue;
        }
        let quantity = row.get("quantity").and_then(Value::as_i64).unwrap_or(0);
        let unit_cost = row
            .remove("unit_cost_cents")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        row.insert("line_total_cents".into(), json!(quantity * unit_cost));
        row.insert("charges_share_cents".into(), json!(0));
    }
}
