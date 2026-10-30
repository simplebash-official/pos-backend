use super::get_database_tables::get_database_tables;
use sqlx::{Column, Row, SqlitePool, TypeInfo, ValueRef};
use std::collections::HashMap;

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
