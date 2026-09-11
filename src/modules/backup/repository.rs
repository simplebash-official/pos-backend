// SQLite repository for full database export and transactional restore.

use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};
use std::collections::HashMap;

/// Explicit list of all persistent tables included in system backups.
/// Excludes temporary caches (e.g. `idempotency_keys`) by design so stale
/// in-flight locks are not restored into a newly restored system.
pub const BACKUP_TABLES: &[&str] = &[
    "categories",
    "subcategories",
    "products",
    "suppliers",
    "supplier_products",
    "purchases",
    "stock_movements",
    "customers",
    "employees",
    "repairs",
    "print_jobs",
    "invoices",
    "payments",
    "credit_notes",
    "product_serials",
    "users",
    "login_sessions",
    "sku_counters",
    "barcode_counters",
    "sequence_counters",
    "sequence_blocks",
    "import_batches",
    "generated_documents",
    "api_keys",
];

/// Exports all rows from each registered SQLite table, dynamically translating
/// column values to JSON objects.
pub(crate) async fn export_all_tables_sqlite(
    pool: &SqlitePool,
) -> Result<HashMap<String, Vec<serde_json::Value>>, sqlx::Error> {
    let mut table_map = HashMap::new();

    for table in BACKUP_TABLES {
        let sql = format!("SELECT * FROM \"{}\"", table);
        let rows = sqlx::query(&sql).fetch_all(pool).await?;
        let mut row_values = Vec::with_capacity(rows.len());

        for row in rows {
            let mut map = serde_json::Map::new();
            for (i, col) in row.columns().iter().enumerate() {
                let name = col.name();
                let raw = match row.try_get_raw(i) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if raw.is_null() {
                    map.insert(name.to_string(), serde_json::Value::Null);
                    continue;
                }
                let type_info = raw.type_info();
                let type_name = type_info.name();
                match type_name {
                    "INTEGER" => {
                        let val: i64 = row.get(i);
                        map.insert(name.to_string(), serde_json::json!(val));
                    }
                    "REAL" => {
                        let val: f64 = row.get(i);
                        map.insert(name.to_string(), serde_json::json!(val));
                    }
                    _ => {
                        let val: String = row.get(i);
                        map.insert(name.to_string(), serde_json::Value::String(val));
                    }
                }
            }
            row_values.push(serde_json::Value::Object(map));
        }

        table_map.insert(table.to_string(), row_values);
    }

    Ok(table_map)
}

/// Restores all tables inside a single atomic transaction. Disables foreign keys
/// during truncation and insertion, then re-enables and validates foreign key
/// constraints before committing to protect against corrupted data.
pub(crate) async fn restore_all_tables_sqlite(
    pool: &SqlitePool,
    tables: &HashMap<String, Vec<serde_json::Value>>,
) -> Result<HashMap<String, usize>, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Disable foreign keys temporarily so tables can be cleared and repopulated
    // regardless of insertion dependency order.
    sqlx::query("PRAGMA foreign_keys = OFF;")
        .execute(&mut *tx)
        .await?;

    // Truncate tables in reverse order.
    for table in BACKUP_TABLES.iter().rev() {
        let sql = format!("DELETE FROM \"{}\";", table);
        sqlx::query(&sql).execute(&mut *tx).await?;
    }

    let mut restored_counts = HashMap::new();

    // Insert records for each table provided in the backup.
    for table in BACKUP_TABLES {
        if let Some(rows) = tables.get(*table) {
            let mut count = 0;
            for row_val in rows {
                if let Some(obj) = row_val.as_object() {
                    if obj.is_empty() {
                        continue;
                    }
                    let cols: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
                    let escaped_cols: Vec<String> =
                        cols.iter().map(|c| format!("\"{}\"", c)).collect();
                    let placeholders: Vec<String> =
                        (1..=cols.len()).map(|i| format!("?{}", i)).collect();
                    let sql = format!(
                        "INSERT INTO \"{}\" ({}) VALUES ({});",
                        table,
                        escaped_cols.join(", "),
                        placeholders.join(", ")
                    );
                    let mut query = sqlx::query(&sql);
                    for col in &cols {
                        let val = &obj[*col];
                        query = match val {
                            serde_json::Value::Null => query.bind(None::<String>),
                            serde_json::Value::Bool(b) => query.bind(if *b { 1i64 } else { 0i64 }),
                            serde_json::Value::Number(n) => {
                                if let Some(i) = n.as_i64() {
                                    query.bind(i)
                                } else if let Some(f) = n.as_f64() {
                                    query.bind(f)
                                } else {
                                    query.bind(None::<String>)
                                }
                            }
                            serde_json::Value::String(s) => query.bind(s.as_str()),
                            serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                                query.bind(val.to_string())
                            }
                        };
                    }
                    query.execute(&mut *tx).await?;
                    count += 1;
                }
            }
            restored_counts.insert(table.to_string(), count);
        }
    }

    // Re-enable foreign keys and verify complete referential integrity before committing.
    sqlx::query("PRAGMA foreign_keys = ON;")
        .execute(&mut *tx)
        .await?;

    let fk_violations: Vec<sqlx::sqlite::SqliteRow> = sqlx::query("PRAGMA foreign_key_check;")
        .fetch_all(&mut *tx)
        .await?;

    if !fk_violations.is_empty() {
        return Err(sqlx::Error::Protocol(format!(
            "Foreign key integrity check failed: {} constraint violation(s) detected",
            fk_violations.len()
        )));
    }

    tx.commit().await?;

    Ok(restored_counts)
}
