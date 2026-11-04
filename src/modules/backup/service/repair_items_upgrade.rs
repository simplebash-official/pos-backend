//! Upgrades a backup taken before repair jobs had items.

use crate::domain::repairs::RepairKind;
use crate::modules::repairs::migration::{MigratedItems, legacy_items};
use serde_json::{Value, json};
use std::collections::HashMap;

/// A backup taken before repair items carries `estimated_cost_cents` and `material_cost_cents`
/// on its repair rows. Rewrites each row, in memory, exactly the way
/// `repairs::migration::migrate_sqlite` rewrites a live database. A row that already has
/// `items` is left as it is; one whose old amounts cannot be read keeps none and fails the
/// restore on the missing columns rather than inventing a price.
pub(super) fn upgrade_pre_repair_items_backup(tables: &mut HashMap<String, Vec<Value>>) {
    for repair in tables.get_mut("repairs").into_iter().flatten() {
        let Some(row) = repair.as_object_mut() else {
            continue;
        };
        if row.contains_key("items") {
            continue;
        }
        let estimated = row.remove("estimated_cost_cents").and_then(|v| v.as_i64());
        let material = row.remove("material_cost_cents").and_then(|v| v.as_i64());
        let (Some(key), Some(issue)) = (
            row.get("key").and_then(Value::as_str),
            row.get("issue_description").and_then(Value::as_str),
        ) else {
            continue;
        };
        let Ok(MigratedItems {
            items,
            price_cents,
            cost_cents,
        }) = legacy_items(key, issue, estimated, material)
        else {
            continue;
        };
        row.insert(
            "kind".into(),
            json!(serde_json::to_string(&RepairKind::Standard).expect("a kind serializes")),
        );
        row.insert(
            "items".into(),
            json!(serde_json::to_string(&items).expect("repair items serialize")),
        );
        row.insert("price_cents".into(), json!(price_cents));
        row.insert("cost_cents".into(), json!(cost_cents));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_repair_row_gets_its_items_and_loses_the_old_amounts() {
        let mut tables = HashMap::from([(
            "repairs".to_string(),
            vec![json!({
                "key": "rep_old", "issue_description": "Broken port",
                "estimated_cost_cents": 400_000, "material_cost_cents": 150_000,
            })],
        )]);
        upgrade_pre_repair_items_backup(&mut tables);
        let row = &tables["repairs"][0];
        assert!(row.get("estimated_cost_cents").is_none());
        assert_eq!(row["price_cents"], 400_000);
        assert_eq!(row["cost_cents"], 150_000);
        let items: Value =
            serde_json::from_str(row["items"].as_str().expect("text")).expect("json");
        assert_eq!(items[0]["key"], "rit_mig_rep_old");
        assert_eq!(items[0]["laborCents"], 400_000);
    }
}
