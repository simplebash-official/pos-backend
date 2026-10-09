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
    let options = with_statement_logging(options)
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

/// Wires sqlx statement logging unless the process started with
/// `LOG_SQL=off`. Which statements actually reach the log is then decided per
/// event by the runtime filter in `core::logging::init`, so a `log_mode`
/// control line can switch between every statement, slow-only and none
/// without reconnecting the pool.
fn with_statement_logging(options: SqliteConnectOptions) -> SqliteConnectOptions {
    use crate::core::logging::{SqlLogging, settings};
    use sqlx::ConnectOptions;
    if settings().sql_at_startup == SqlLogging::Off {
        return options.disable_statement_logging();
    }
    options
        .log_statements(log::LevelFilter::Info)
        .log_slow_statements(
            log::LevelFilter::Warn,
            std::time::Duration::from_millis(250),
        )
}

/// Runs embedded DDL schema migrations to create all tables and indexes,
/// followed by automatic schema healing/migrations for existing databases.
pub async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Run migrations first so legacy tables have required columns before SCHEMA_SQL creates indexes on them
    migrate_schema(pool).await?;
    pool.execute(SCHEMA_SQL).await?;
    migrate_schema(pool).await?;
    // Sync v2 device bookkeeping: the single state row and the capture
    // triggers. Both are inert until sync is enabled (`capture_enabled = 1`).
    super::sync_capture::ensure_state(pool)
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    super::sync_capture::install_triggers(pool)
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    tracing::info!("SQLite database schema and indexes initialized");
    Ok(())
}

/// Applies incremental migrations for legacy databases (e.g., removing deprecated columns).
pub async fn migrate_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Check if 'login_sessions' table has a legacy 'user_id' column from earlier revisions
    let columns: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('login_sessions')")
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    if columns.iter().any(|(_, name)| name == "user_id") {
        tracing::info!("Migrating legacy column 'user_id' from 'login_sessions' table");
        let _ = pool
            .execute("ALTER TABLE login_sessions DROP COLUMN user_id")
            .await;
    }

    // `sync_state.outbox_epoch` (see `sync_capture::ensure_state`) arrived after
    // the table did.
    let sync_state_cols: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('sync_state')")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    if !sync_state_cols.is_empty() && !sync_state_cols.iter().any(|(_, n)| n == "outbox_epoch") {
        let _ = pool
            .execute("ALTER TABLE sync_state ADD COLUMN outbox_epoch TEXT")
            .await;
    }

    migrate_users_to_usernames(pool).await?;
    migrate_login_sessions_to_usernames(pool).await?;

    // `users.preferences_json` (per-login UI preferences) arrived after the table did.
    let user_cols: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('users')")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    if !user_cols.is_empty() && !user_cols.iter().any(|(_, n)| n == "preferences_json") {
        tracing::info!("Migrating 'users' table: adding 'preferences_json' column");
        let _ = pool
            .execute("ALTER TABLE users ADD COLUMN preferences_json TEXT")
            .await;
    }

    // Check if 'generated_documents' table is missing required columns from earlier schema versions
    let gen_doc_cols: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('generated_documents')")
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    if !gen_doc_cols.is_empty() {
        if !gen_doc_cols.iter().any(|(_, name)| name == "entity_key") {
            tracing::info!(
                "Migrating 'generated_documents' table: adding missing 'entity_key' column"
            );
            let _ = pool
                .execute(
                    "ALTER TABLE generated_documents ADD COLUMN entity_key TEXT NOT NULL DEFAULT ''",
                )
                .await;
        }
        if !gen_doc_cols.iter().any(|(_, name)| name == "document_type") {
            tracing::info!(
                "Migrating 'generated_documents' table: adding missing 'document_type' column"
            );
            let _ = pool
                .execute(
                    "ALTER TABLE generated_documents ADD COLUMN document_type TEXT NOT NULL DEFAULT ''",
                )
                .await;
        }
        if !gen_doc_cols
            .iter()
            .any(|(_, name)| name == "file_size_bytes")
        {
            tracing::info!(
                "Migrating 'generated_documents' table: adding missing 'file_size_bytes' column"
            );
            let _ = pool
                .execute(
                    "ALTER TABLE generated_documents ADD COLUMN file_size_bytes INTEGER NOT NULL DEFAULT 0",
                )
                .await;
        }
        let _ = pool
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_generated_documents_lookup ON generated_documents(entity_key, document_type, created_at DESC)",
            )
            .await;
    }

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

/// A login username derived from a legacy email's local part: lowercased,
/// anything outside `a-z0-9._-` becomes `-`, edges trimmed to a letter or
/// digit, padded with digits to the 3-character minimum, cut to 32, then made
/// unique against `taken` with a numeric suffix. Used only by the one-off
/// email -> username migration below.
fn derive_username(email: &str, taken: &std::collections::HashSet<String>) -> String {
    let local = email.split('@').next().unwrap_or_default().to_lowercase();
    let cleaned: String = local
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    let mut base = cleaned
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_string();
    base.truncate(28);
    let mut n = 1;
    while base.chars().count() < 3 {
        base.push_str(&n.to_string());
        n += 1;
    }
    let mut candidate = base.clone();
    let mut suffix = 2;
    while taken.contains(&candidate) {
        candidate = format!("{base}{suffix}");
        suffix += 1;
    }
    candidate
}

/// One-off rebuild of a desktop database whose `users` table still logs in by
/// `email`: the Admin becomes `admin`, every other login gets a username derived
/// from its email (see `derive_username`), `email` is dropped, and the audit
/// column `login_sessions.email_at_login` is renamed. There is no email fallback
/// afterwards. A no-op once the table has `username`.
async fn migrate_users_to_usernames(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    use sqlx::Row;

    let cols: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('users')")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    if !cols.iter().any(|(_, n)| n == "email") {
        return Ok(());
    }
    tracing::info!("Migrating 'users' table: replacing 'email' with 'username'");

    let has_prefs = cols.iter().any(|(_, n)| n == "preferences_json");
    let rows = sqlx::query("SELECT key, email, role FROM users ORDER BY created_at, key")
        .fetch_all(pool)
        .await?;

    let mut taken = std::collections::HashSet::new();
    let mut assigned: Vec<(String, String)> = Vec::new();
    // The Admin first, so no other login can claim `admin`.
    for row in rows
        .iter()
        .filter(|r| r.get::<String, _>("role") == "admin")
    {
        taken.insert("admin".to_string());
        assigned.push((row.get("key"), "admin".to_string()));
    }
    for row in rows
        .iter()
        .filter(|r| r.get::<String, _>("role") != "admin")
    {
        let username = derive_username(&row.get::<String, _>("email"), &taken);
        taken.insert(username.clone());
        assigned.push((row.get("key"), username));
    }

    let mut tx = pool.begin().await?;
    sqlx::query("ALTER TABLE users ADD COLUMN username TEXT")
        .execute(&mut *tx)
        .await?;
    for (key, username) in &assigned {
        sqlx::query("UPDATE users SET username = $1 WHERE key = $2")
            .bind(username)
            .bind(key)
            .execute(&mut *tx)
            .await?;
    }
    // SQLite cannot drop a UNIQUE column, so rebuild the table around it.
    sqlx::query(
        r#"CREATE TABLE users_new (
            key TEXT PRIMARY KEY,
            id TEXT NOT NULL,
            name TEXT NOT NULL,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL,
            employee_key TEXT,
            preferences_json TEXT,
            is_active INTEGER NOT NULL DEFAULT 1,
            version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            deleted_at TEXT,
            updated_by_device TEXT
        )"#,
    )
    .execute(&mut *tx)
    .await?;
    let prefs = if has_prefs {
        "preferences_json"
    } else {
        "NULL"
    };
    sqlx::query(&format!(
        "INSERT INTO users_new (key, id, name, username, password_hash, role, employee_key, \
         preferences_json, is_active, version, created_at, updated_at, deleted_at, updated_by_device) \
         SELECT key, id, name, username, password_hash, role, employee_key, {prefs}, is_active, \
         version, created_at, updated_at, deleted_at, updated_by_device FROM users"
    ))
    .execute(&mut *tx)
    .await?;
    sqlx::query("DROP TABLE users").execute(&mut *tx).await?;
    sqlx::query("ALTER TABLE users_new RENAME TO users")
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

/// Renames the login-session audit column `email_at_login` to `username_at_login`
/// and refreshes each row from its user when that user still exists.
async fn migrate_login_sessions_to_usernames(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let session_cols: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('login_sessions')")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    if !session_cols.iter().any(|(_, n)| n == "email_at_login") {
        return Ok(());
    }
    sqlx::query("ALTER TABLE login_sessions RENAME COLUMN email_at_login TO username_at_login")
        .execute(pool)
        .await?;
    let user_cols: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('users')")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    if user_cols.iter().any(|(_, n)| n == "username") {
        sqlx::query(
            "UPDATE login_sessions SET username_at_login = COALESCE( \
             (SELECT username FROM users WHERE users.key = login_sessions.user_key), \
             username_at_login)",
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod username_migration_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn derives_clean_unique_usernames() {
        let mut taken = HashSet::from(["admin".to_string()]);
        assert_eq!(derive_username("Ann.Lee@shop.com", &taken), "ann.lee");
        taken.insert("ann.lee".into());
        assert_eq!(derive_username("ann.lee@other.com", &taken), "ann.lee2");
        assert_eq!(derive_username("a@x.com", &taken), "a12");
        assert_eq!(derive_username("+bob+@x.com", &taken), "bob");
    }
}
