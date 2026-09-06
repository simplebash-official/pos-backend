// SQLite client configuration, connection pooling, and schema bootstrap.

use sqlx::{
    Executor, SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::path::Path;

const SCHEMA_SQL: &str = include_str!("sqlite_schema.sql");

/// Initializes the SQLite connection pool, creates parent directories if needed,
/// applies optimal concurrency PRAGMAs, and runs initial schema migrations.
pub async fn connect(url: &str) -> Result<SqlitePool, sqlx::Error> {
    // If the URL points to a file, ensure its parent directory exists
    if let Some(path_str) = url.strip_prefix("sqlite://") {
        let clean_path = path_str.split('?').next().unwrap_or(path_str);
        if clean_path != ":memory:" && !clean_path.is_empty() {
            let path = Path::new(clean_path);
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                let _ = std::fs::create_dir_all(parent);
            }
        }
    }

    let options: SqliteConnectOptions = url.parse()?;
    let options = options
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(std::time::Duration::from_secs(5))
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect_with(options)
        .await?;

    init_db(&pool).await?;

    Ok(pool)
}

/// Runs embedded DDL schema migrations to create all tables and indexes.
pub async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    pool.execute(SCHEMA_SQL).await?;
    tracing::info!("SQLite database schema and indexes initialized");
    Ok(())
}

/// Generates a 24-character hexadecimal identifier matching MongoDB ObjectId format.
pub fn generate_id_hex() -> String {
    mongodb::bson::oid::ObjectId::new().to_hex()
}

/// Formats the current UTC timestamp as an ISO-8601/RFC-3339 string.
pub fn now_utc_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Parses an RFC-3339 timestamp string into chrono `DateTime<Utc>`.
pub fn parse_iso_datetime(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}

/// Converts an RFC-3339 string directly into `mongodb::bson::DateTime`.
pub fn to_bson_datetime(s: &str) -> mongodb::bson::DateTime {
    mongodb::bson::DateTime::from_chrono(parse_iso_datetime(s))
}

/// Formats a `mongodb::bson::DateTime` into an RFC-3339 string.
pub fn bson_to_iso(dt: &mongodb::bson::DateTime) -> String {
    dt.to_chrono().to_rfc3339()
}

use sqlx::{Column, Row, TypeInfo, ValueRef};

/// Converts a generic SQLite row into a MongoDB BSON `Document`.
/// Maps ISO date strings to BSON DateTime, JSON strings to BSON documents/arrays,
/// 24-hex IDs to BSON ObjectId, and known boolean flags to BSON Boolean.
pub fn map_sqlite_row_to_document(row: &sqlx::sqlite::SqliteRow) -> mongodb::bson::Document {
    let mut doc = mongodb::bson::Document::new();
    for (i, col) in row.columns().iter().enumerate() {
        let name = col.name();
        let raw = match row.try_get_raw(i) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if raw.is_null() {
            continue;
        }
        let type_info = raw.type_info();
        let type_name = type_info.name();
        match type_name {
            "INTEGER" => {
                let val: i64 = row.get(i);
                if matches!(
                    name,
                    "is_serialized"
                        | "is_credit"
                        | "no_receipt"
                        | "is_manager_override"
                        | "is_active"
                ) {
                    doc.insert(name, mongodb::bson::Bson::Boolean(val != 0));
                } else {
                    doc.insert(name, mongodb::bson::Bson::Int64(val));
                }
            }
            "REAL" => {
                let val: f64 = row.get(i);
                doc.insert(name, mongodb::bson::Bson::Double(val));
            }
            "TEXT" => {
                let s: String = row.get(i);
                if name == "id" {
                    if let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(&s) {
                        doc.insert("_id", mongodb::bson::Bson::ObjectId(oid));
                        doc.insert("id", mongodb::bson::Bson::ObjectId(oid));
                    } else {
                        doc.insert(name, mongodb::bson::Bson::String(s));
                    }
                } else if name == "product_id" {
                    if let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(&s) {
                        doc.insert(name, mongodb::bson::Bson::ObjectId(oid));
                    } else {
                        doc.insert(name, mongodb::bson::Bson::String(s));
                    }
                } else if name.ends_with("_at") || name.ends_with("_date") || name == "date" {
                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                        doc.insert(
                            name,
                            mongodb::bson::Bson::DateTime(mongodb::bson::DateTime::from_chrono(
                                dt.with_timezone(&chrono::Utc),
                            )),
                        );
                    } else {
                        doc.insert(name, mongodb::bson::Bson::String(s));
                    }
                } else if (s.starts_with('{') && s.ends_with('}'))
                    || (s.starts_with('[') && s.ends_with(']'))
                {
                    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&s) {
                        doc.insert(name, json_to_bson(json_val));
                    } else {
                        doc.insert(name, mongodb::bson::Bson::String(s));
                    }
                } else {
                    doc.insert(name, mongodb::bson::Bson::String(s));
                }
            }
            _ => {}
        }
    }
    doc
}

fn json_to_bson(v: serde_json::Value) -> mongodb::bson::Bson {
    match v {
        serde_json::Value::Null => mongodb::bson::Bson::Null,
        serde_json::Value::Bool(b) => mongodb::bson::Bson::Boolean(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                mongodb::bson::Bson::Int64(i)
            } else if let Some(f) = n.as_f64() {
                mongodb::bson::Bson::Double(f)
            } else {
                mongodb::bson::Bson::Null
            }
        }
        serde_json::Value::String(s) => mongodb::bson::Bson::String(s),
        serde_json::Value::Array(arr) => {
            mongodb::bson::Bson::Array(arr.into_iter().map(json_to_bson).collect())
        }
        serde_json::Value::Object(map) => {
            let mut doc = mongodb::bson::Document::new();
            for (k, val) in map {
                doc.insert(k, json_to_bson(val));
            }
            mongodb::bson::Bson::Document(doc)
        }
    }
}
