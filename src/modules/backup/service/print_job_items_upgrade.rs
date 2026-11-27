//! Upgrades a backup taken before print jobs had items.

use crate::domain::print_jobs::PrintJobKind;
use crate::modules::print_jobs::migration::{LegacyPrintJob, MigratedItems, legacy_items};
use serde_json::{Value, json};
use std::collections::HashMap;

/// A backup taken before print job items carries `estimated_cost_cents` and
/// `material_cost_cents` on its print job rows. Rewrites each row, in memory, exactly the way
/// `print_jobs::migration::migrate_items_sqlite` rewrites a live database. A row that already
/// has `items` is left as it is; one whose old amounts cannot be read keeps none and fails the
/// restore on the missing columns rather than inventing a price.
pub(super) fn upgrade_pre_print_job_items_backup(tables: &mut HashMap<String, Vec<Value>>) {
    for job in tables.get_mut("print_jobs").into_iter().flatten() {
        let Some(row) = job.as_object_mut() else {
            continue;
        };
        if row.contains_key("items") {
            continue;
        }
        let (Some(key), Some(job_type), Some(quantity), Some(estimated_cost_cents)) = (
            row.get("key").and_then(Value::as_str),
            row.get("job_type").and_then(Value::as_str),
            row.get("quantity").and_then(Value::as_i64),
            row.get("estimated_cost_cents").and_then(Value::as_i64),
        ) else {
            continue;
        };
        let material_cost_cents = match row.get("material_cost_cents") {
            None | Some(Value::Null) => None,
            Some(stored) => match stored.as_i64() {
                Some(cents) => Some(cents),
                None => continue,
            },
        };
        let Ok(MigratedItems {
            items,
            price_cents,
            cost_cents,
        }) = legacy_items(LegacyPrintJob {
            key,
            job_type,
            quantity,
            estimated_cost_cents,
            material_cost_cents,
        })
        else {
            continue;
        };
        row.remove("estimated_cost_cents");
        row.remove("material_cost_cents");
        row.insert(
            "kind".into(),
            json!(serde_json::to_string(&PrintJobKind::Standard).expect("a kind serializes")),
        );
        row.insert(
            "items".into(),
            json!(serde_json::to_string(&items).expect("print job items serialize")),
        );
        row.insert("price_cents".into(), json!(price_cents));
        row.insert("cost_cents".into(), json!(cost_cents));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_print_row_gets_its_items_and_loses_the_old_amounts() {
        let mut tables = HashMap::from([(
            "print_jobs".to_string(),
            vec![json!({
                "key": "prn_old", "job_type": "t-shirt", "quantity": 20,
                "estimated_cost_cents": 400_000, "material_cost_cents": 150_000,
            })],
        )]);
        upgrade_pre_print_job_items_backup(&mut tables);
        let row = &tables["print_jobs"][0];
        assert!(row.get("estimated_cost_cents").is_none());
        assert!(row.get("material_cost_cents").is_none());
        assert_eq!(row["price_cents"], 400_000);
        assert_eq!(row["cost_cents"], 150_000);
        let items: Value =
            serde_json::from_str(row["items"].as_str().expect("text")).expect("json");
        assert_eq!(items[0]["key"], "pit_mig_prn_old");
        assert_eq!(items[0]["unitPriceCents"], 400_000);
        assert_eq!(items[1]["key"], "pit_mig_mat_prn_old");
        assert_eq!(items[1]["costCents"], 150_000);
    }

    #[test]
    fn a_row_with_unreadable_amounts_is_left_for_the_restore_to_refuse() {
        let broken = json!({
            "key": "prn_bad", "job_type": "mug", "quantity": 1,
            "estimated_cost_cents": 10, "material_cost_cents": "ten",
        });
        let mut tables = HashMap::from([("print_jobs".to_string(), vec![broken.clone()])]);
        upgrade_pre_print_job_items_backup(&mut tables);
        assert_eq!(tables["print_jobs"][0], broken);
    }
}
