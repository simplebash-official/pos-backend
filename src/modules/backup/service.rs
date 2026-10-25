// Service orchestration and business validation for full data backup and restore.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::{
    app::AppState,
    clients::db::Db,
    core::constants::prefixes::opening_batch_key,
    core::error::{AppError, AppResult},
    domain::backup::{
        BackupExportData, ExportBackupRequest, RestoreBackupRequest, RestoreBackupResponse,
    },
    modules::{backup::repository, inventory::service::stock_batch::backfill_line_allocations},
};

/// Supported backup file schema version.
const BACKUP_SCHEMA_VERSION: u32 = 1;

/// Exports all SQLite tables and optional client settings into a unified backup object.
pub(crate) async fn export_backup(
    db: &Db,
    req: ExportBackupRequest,
    app_version: &str,
) -> AppResult<BackupExportData> {
    crate::core::logging::domain::tracked("backup.exported", async move {
        let pool = db.as_sqlite().ok_or_else(|| {
            AppError::validation(
                "Data backup export is currently only supported in SQLite desktop mode",
            )
        })?;

        let tables = repository::export_all_tables_sqlite(pool)
            .await
            .map_err(|e| AppError::internal(format!("Failed to export database tables: {e}")))?;

        let total_tables = tables.len();
        let total_records: usize = tables.values().map(|rows| rows.len()).sum();

        let settings = if req.include_settings {
            req.settings
        } else {
            None
        };

        Ok(BackupExportData {
            version: BACKUP_SCHEMA_VERSION,
            exported_at: chrono::Utc::now().to_rfc3339(),
            app_version: app_version.to_string(),
            environment: "desktop".to_string(),
            total_tables,
            total_records,
            tables,
            settings,
        })
    })
    .await
}

/// Validates and transactionally restores database tables from a backup object.
pub(crate) async fn restore_backup(
    state: &AppState,
    req: RestoreBackupRequest,
) -> AppResult<RestoreBackupResponse> {
    crate::core::logging::domain::tracked("backup.restored", async move {
        let mut backup = req.backup;
        upgrade_pre_batch_backup(&mut backup.tables);
        upgrade_pre_receipt_backup(&mut backup.tables);
        upgrade_pre_line_total_backup(&mut backup.tables);

        if backup.version != BACKUP_SCHEMA_VERSION {
            return Err(AppError::validation(format!(
                "Unsupported backup schema version: {} (expected {})",
                backup.version, BACKUP_SCHEMA_VERSION
            )));
        }

        let pool = state.db.as_sqlite().ok_or_else(|| {
            AppError::validation("Data restore is currently only supported in SQLite desktop mode")
        })?;

        let restored_counts = repository::restore_all_tables_sqlite(pool, &backup.tables)
            .await
            .map_err(|e| {
                AppError::validation(format!("Failed to restore database from backup: {e}"))
            })?;

        let total_tables = restored_counts.len();
        let total_records: usize = restored_counts.values().sum();

        // Invalidate active report caches so live dashboards immediately reflect the restored state.
        state.reports_engine.invalidate_active();

        Ok(RestoreBackupResponse {
            restored_at: chrono::Utc::now().to_rfc3339(),
            total_tables,
            total_records,
            restored_counts,
            settings: backup.settings,
        })
    })
    .await
}

/// A backup taken before lot prices and extra charges prices each purchase line per unit
/// (`unit_cost_cents`); today's schema holds the line's total (`line_total_cents`) and its share
/// of extra charges. Rewrites such lines, in memory, exactly the way
/// `clients::sqlite::migrate_schema` rewrites a live database: the total is quantity x the old
/// unit cost and there is no share. A backup whose lines already have a total is left untouched.
fn upgrade_pre_line_total_backup(tables: &mut HashMap<String, Vec<Value>>) {
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

/// A backup taken before supplier receipts carries purchases with no `receipt_key`, which
/// today's schema requires. Rewrites such a backup, in memory, exactly the way
/// `clients::sqlite::migrate_purchases_to_receipts` rewrites a live database: purchases of one
/// supplier, invoice number and day share a receipt keyed by the smallest purchase key; a
/// purchase without an invoice number is a receipt of its own. A backup whose purchases all
/// have a receipt is left untouched.
fn upgrade_pre_receipt_backup(tables: &mut HashMap<String, Vec<Value>>) {
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
            let tail: String = first.chars().rev().take(10).collect::<Vec<_>>().into_iter().rev().collect();
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

/// A backup taken before stock batches existed carries each product's price
/// on the product row and no `stock_batches` table, while today's schema
/// needs a batch behind every unit (and a batch key on serials, a price on
/// purchases). Rewrites such a backup, in memory, exactly the way
/// `clients::sqlite::migrate_to_stock_batches` rewrites a live database: one
/// opening batch per product holding its stock at its last cost and price,
/// movements/serials pointed at it, purchases priced, sale lines allocated.
/// A backup that already has batches is left untouched.
fn upgrade_pre_batch_backup(tables: &mut HashMap<String, Vec<Value>>) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pre_batch_backup_gets_opening_batches_before_restore() {
        let mut tables: HashMap<String, Vec<Value>> = HashMap::new();
        tables.insert(
            "products".into(),
            vec![
                json!({ "key": "prod_a", "id": "aaa", "cost_price_cents": 100_000,
                         "selling_price_cents": 120_000, "stock_quantity": 5,
                         "created_at": "2026-01-01T00:00:00Z", "deleted_at": null }),
            ],
        );
        tables.insert(
            "stock_movements".into(),
            vec![json!({ "key": "sm_1", "product_id": "aaa", "quantity_delta": 5 })],
        );
        tables.insert(
            "product_serials".into(),
            vec![json!({ "key": "psn_1", "product_key": "prod_a" })],
        );
        tables.insert(
            "purchases".into(),
            vec![json!({ "key": "pur_1", "product_key": "prod_a" })],
        );
        tables.insert(
            "invoices".into(),
            vec![json!({ "key": "inv_1",
                         "items": "[{\"productKey\":\"prod_a\",\"quantity\":2}]" })],
        );

        upgrade_pre_batch_backup(&mut tables);

        let batch = &tables["stock_batches"][0];
        assert_eq!(batch["key"], "stb_open_prod_a");
        assert_eq!(batch["remaining_quantity"], 5);
        assert_eq!(batch["selling_price_cents"], 120_000);
        assert_eq!(tables["stock_movements"][0]["batch_key"], "stb_open_prod_a");
        assert_eq!(tables["product_serials"][0]["batch_key"], "stb_open_prod_a");
        assert_eq!(tables["purchases"][0]["selling_price_cents"], 120_000);
        let items: Value =
            serde_json::from_str(tables["invoices"][0]["items"].as_str().unwrap()).unwrap();
        assert_eq!(items[0]["batchAllocations"][0]["unitCostCents"], 100_000);
    }

    #[test]
    fn purchases_of_a_pre_receipt_backup_are_grouped_into_receipts() {
        let mut tables: HashMap<String, Vec<Value>> = HashMap::new();
        tables.insert(
            "purchases".into(),
            vec![
                json!({ "key": "pur_b", "supplier_key": "sup_1", "reference_no": "INV-1", "date": "2026-01-01T00:00:00Z", "created_at": "2026-01-01T00:00:00Z" }),
                json!({ "key": "pur_a", "supplier_key": "sup_1", "reference_no": " INV-1 ", "date": "2026-01-01T00:00:00Z", "created_at": "2026-01-01T00:00:00Z" }),
                json!({ "key": "pur_c", "supplier_key": "sup_1", "date": "2026-01-01T00:00:00Z", "created_at": "2026-01-01T00:00:00Z" }),
            ],
        );

        upgrade_pre_receipt_backup(&mut tables);

        let purchases = &tables["purchases"];
        assert_eq!(purchases[0]["receipt_key"], "prc_leg_pur_a");
        assert_eq!(purchases[1]["receipt_key"], "prc_leg_pur_a");
        assert_eq!(purchases[2]["receipt_key"], "prc_leg_pur_c");
        assert!(purchases[0].get("reference_no").is_none());
        let receipts = &tables["purchase_receipts"];
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0]["reference_no"], "INV-1");
        assert_eq!(receipts[0]["receipt_number"], "PUR-Lpur_a");
    }

    #[test]
    fn purchase_lines_priced_per_unit_get_a_line_total() {
        let mut tables: HashMap<String, Vec<Value>> = HashMap::new();
        tables.insert(
            "purchases".into(),
            vec![
                json!({ "key": "pur_a", "quantity": 4, "unit_cost_cents": 250 }),
                json!({ "key": "pur_b", "quantity": 2, "line_total_cents": 900, "charges_share_cents": 10 }),
            ],
        );
        upgrade_pre_line_total_backup(&mut tables);
        let purchases = &tables["purchases"];
        assert_eq!(purchases[0]["line_total_cents"], 1000);
        assert_eq!(purchases[0]["charges_share_cents"], 0);
        assert!(purchases[0].get("unit_cost_cents").is_none());
        assert_eq!(purchases[1]["line_total_cents"], 900);
        assert_eq!(purchases[1]["charges_share_cents"], 10);
    }

    #[test]
    fn a_backup_with_batches_is_left_alone() {
        let mut tables: HashMap<String, Vec<Value>> = HashMap::new();
        tables.insert(
            "products".into(),
            vec![json!({ "key": "prod_a", "id": "aaa", "stock_quantity": 5 })],
        );
        let before = tables.clone();
        upgrade_pre_batch_backup(&mut tables);
        assert_eq!(tables, before);
    }
}
