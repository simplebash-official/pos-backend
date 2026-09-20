// SQLite repository for full database export and transactional restore.

use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};
use std::collections::{HashMap, HashSet};

/// Tables excluded from backups by design (ephemeral locks, query caches, migrations metadata).
const EXCLUDED_TABLES: &[&str] = &["idempotency_keys", "_sqlx_migrations"];

/// A table that must not travel in a backup: the fixed exclusions plus every
/// `sync_*` bookkeeping table. Restoring another install's sync state (device
/// id, outbox, merge metadata) would make two devices share an identity.
fn is_excluded(name: &str) -> bool {
    EXCLUDED_TABLES.contains(&name) || name.starts_with("sync_")
}

/// Discovers all persistent user/application tables in the SQLite database dynamically.
pub(crate) async fn get_database_tables(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%' ORDER BY name;",
    )
    .fetch_all(pool)
    .await?;

    let tables: Vec<String> = rows
        .into_iter()
        .map(|r| r.get::<String, _>(0))
        .filter(|name| !is_excluded(name))
        .collect();

    Ok(tables)
}

/// Discovers all persistent user/application tables within an active transaction.
async fn get_database_tables_tx(
    tx: &mut sqlx::SqliteConnection,
) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%' ORDER BY name;",
    )
    .fetch_all(&mut *tx)
    .await?;

    let tables: Vec<String> = rows
        .into_iter()
        .map(|r| r.get::<String, _>(0))
        .filter(|name| !is_excluded(name))
        .collect();

    Ok(tables)
}

/// Fetches the set of existing column names for a given table within an active transaction.
async fn get_table_columns_tx(
    table: &str,
    tx: &mut sqlx::SqliteConnection,
) -> Result<HashSet<String>, sqlx::Error> {
    let sql = format!("PRAGMA table_info(\"{}\");", table);
    let rows = sqlx::query(&sql).fetch_all(&mut *tx).await?;
    let cols = rows.into_iter().map(|r| r.get::<String, _>(1)).collect();
    Ok(cols)
}

/// Exports all rows from each discovered SQLite table, dynamically translating
/// column values to JSON objects.
pub(crate) async fn export_all_tables_sqlite(
    pool: &SqlitePool,
) -> Result<HashMap<String, Vec<serde_json::Value>>, sqlx::Error> {
    let tables = get_database_tables(pool).await?;
    let mut table_map = HashMap::new();

    for table in &tables {
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

        table_map.insert(table.clone(), row_values);
    }

    Ok(table_map)
}

/// Restores all tables inside a single atomic transaction. Disables foreign keys
/// during truncation and insertion, then re-enables and validates foreign key
/// constraints before committing to protect against corrupted data.
///
/// Supports future schema evolution:
/// - Any table in the backup that no longer exists in the current database is skipped gracefully.
/// - Any table in the current database that has data in the backup is truncated and restored.
/// - Only columns that currently exist in SQLite are inserted; columns that were deprecated/removed
///   are ignored, and newly added columns receive their default value or NULL.
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

    // Discover current tables in the database schema.
    let current_tables = get_database_tables_tx(&mut tx).await?;
    let current_tables_set: HashSet<String> = current_tables.iter().cloned().collect();

    // Truncate existing tables in reverse order.
    for table in current_tables.iter().rev() {
        let sql = format!("DELETE FROM \"{}\";", table);
        sqlx::query(&sql).execute(&mut *tx).await?;
    }

    let mut restored_counts = HashMap::new();

    // Insert records for each table provided in the backup that exists in the database.
    for (table, rows) in tables {
        if !current_tables_set.contains(table) {
            tracing::warn!(
                "Table '{}' from backup does not exist in current database schema; skipping.",
                table
            );
            continue;
        }

        let live_columns = get_table_columns_tx(table, &mut tx).await?;
        let mut count = 0;

        for row_val in rows {
            if let Some(obj) = row_val.as_object() {
                if obj.is_empty() {
                    continue;
                }
                // Filter to only columns that actually exist in the current database table.
                let cols: Vec<&str> = obj
                    .keys()
                    .map(|k| k.as_str())
                    .filter(|k| live_columns.contains(*k))
                    .collect();

                if cols.is_empty() {
                    continue;
                }

                let escaped_cols: Vec<String> = cols.iter().map(|c| format!("\"{}\"", c)).collect();
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
        restored_counts.insert(table.clone(), count);
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
