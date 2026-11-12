use super::migrate_schema::migrate_schema;
use crate::core::error::{AppError, AppResult};
use crate::core::logging::SqlLogging;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Column, Executor, Row, SqlitePool, TypeInfo, ValueRef};
use std::path::Path;

/// Formats a `mongodb::bson::DateTime` into an RFC-3339 string.
#[must_use]
pub fn bson_to_iso(dt: &mongodb::bson::DateTime) -> String {
    dt.to_chrono().to_rfc3339()
}

/// Initializes the SQLite connection pool, creates parent directories if needed,
/// applies optimal concurrency PRAGMAs, and runs initial schema migrations.
pub async fn connect(url: &str, sql_logging: SqlLogging) -> Result<SqlitePool, sqlx::Error> {
    // If the URL points to a file, ensure its parent directory exists
    if let Some(path_str) = url.strip_prefix("sqlite://") {
        let clean_path = path_str
            .split('?')
            .next()
            .expect("split yields at least one item");
        if clean_path != ":memory:" && !clean_path.is_empty() {
            let path = Path::new(clean_path);
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent).map_err(sqlx::Error::Io)?;
            }
        }
    }

    let options: SqliteConnectOptions = url.parse()?;
    let options = with_statement_logging(options, sql_logging)
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
/// `LOG_SQL=off` (`sql_logging`). Which statements actually reach the log is then decided per
/// event by the runtime filter in `core::logging::init`, so a `log_mode`
/// control line can switch between every statement, slow-only and none
/// without reconnecting the pool.
fn with_statement_logging(
    options: SqliteConnectOptions,
    sql_logging: SqlLogging,
) -> SqliteConnectOptions {
    use sqlx::ConnectOptions;
    if sql_logging == SqlLogging::Off {
        return options.disable_statement_logging();
    }
    options
        .log_statements(log::LevelFilter::Info)
        .log_slow_statements(
            log::LevelFilter::Warn,
            std::time::Duration::from_millis(250),
        )
}

/// A login username derived from a legacy email's local part: lowercased,
/// anything outside `a-z0-9._-` becomes `-`, edges trimmed to a letter or
/// digit, padded with digits to the 3-character minimum, cut to 32, then made
/// unique against `taken` with a numeric suffix. Used only by the one-off
/// email -> username migration below.
pub(super) fn derive_username(email: &str, taken: &std::collections::HashSet<String>) -> String {
    let local = email
        .split('@')
        .next()
        .expect("split yields at least one item")
        .to_lowercase();
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

/// Generates a 24-character hexadecimal identifier matching MongoDB ObjectId format.
#[must_use]
pub fn generate_id_hex() -> String {
    mongodb::bson::oid::ObjectId::new().to_hex()
}

const SCHEMA_SQL: &str = include_str!("../sqlite_schema.sql");

/// Runs embedded DDL schema migrations to create all tables and indexes,
/// followed by automatic schema healing/migrations for existing databases.
pub async fn init_db(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // Run migrations first so legacy tables have required columns before SCHEMA_SQL creates indexes on them
    migrate_schema(pool).await?;
    pool.execute(SCHEMA_SQL).await?;
    migrate_schema(pool).await?;
    migrate_purchases_to_receipts(pool).await?;
    crate::modules::warranties::migration::migrate_sqlite(pool).await?;
    crate::modules::repairs::migration::migrate_sqlite(pool).await?;
    // Sync v2 device bookkeeping: the single state row and the capture
    // triggers. Both are inert until sync is enabled (`capture_enabled = 1`).
    super::super::sync_capture::ensure_state(pool)
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    super::super::sync_capture::install_triggers(pool)
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    tracing::info!("SQLite database schema and indexes initialized");
    Ok(())
}

/// Converts a generic SQLite row into a MongoDB BSON `Document`.
/// Maps ISO date strings to BSON DateTime, JSON strings to BSON documents/arrays,
/// 24-hex IDs to BSON ObjectId, and known boolean flags to BSON Boolean.
#[must_use]
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
                        | "has_out_of_stock_items"
                        | "allow_shared_user_sessions"
                        | "allow_staff_multi_device_sessions"
                        | "has_late_movements"
                        | "recorded_late"
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

/// One-time move of `cash_movements` from the six-type, free-text-reason shape to
/// the typed shape with categories: old types map to the new ones, `reason` becomes
/// `note`, the old payee text becomes `payee_name` (and a supplier key a `payee_key`),
/// and every row is filed under its type's seeded "Other" category (deterministic
/// keys, so a desktop and the cloud migrating separately write identical rows).
/// The old table is rebuilt because `reason` was NOT NULL. Capture triggers are
/// dropped first (`init_db` reinstalls them) so the move is not queued for upload.
/// Runs on the first pass (before `SCHEMA_SQL`, which indexes the new columns); the
/// category rows these keys point at are seeded at startup.
pub async fn migrate_cash_movements(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    use crate::domain::cash_categories::category_key;
    use crate::domain::sessions::CashMovementType as T;

    let columns = table_columns(pool, "cash_movements").await?;
    if !columns.iter().any(|c| c == "reason") {
        return Ok(());
    }
    tracing::info!("Migrating cash movements to typed movements with categories");

    let other = |t: T| category_key(t, "Other");
    let mut tx = pool.begin().await?;
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_all(&mut *tx)
    .await?;
    for trigger in triggers {
        tx.execute(format!("DROP TRIGGER IF EXISTS \"{trigger}\"").as_str())
            .await?;
    }

    tx.execute("ALTER TABLE cash_movements RENAME TO cash_movements_old")
        .await?;
    tx.execute(
        "CREATE TABLE cash_movements (
            key TEXT PRIMARY KEY, id TEXT NOT NULL, movement_number TEXT NOT NULL UNIQUE,
            movement_type TEXT NOT NULL, direction TEXT NOT NULL, amount_cents INTEGER NOT NULL,
            session_key TEXT, terminal_id TEXT NOT NULL, category_key TEXT NOT NULL,
            custom_label TEXT, payee_key TEXT, payee_name TEXT, period_start TEXT, period_end TEXT,
            reference_no TEXT, note TEXT, recorded_by_user_id TEXT NOT NULL,
            recorded_by_name_snapshot TEXT NOT NULL, approved_by_user_id TEXT,
            approved_by_name_snapshot TEXT, recorded_late INTEGER NOT NULL DEFAULT 0,
            original_variance_session_key TEXT, version INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT,
            updated_by_device TEXT
        )",
    )
    .await?;
    // The old indexes went with the old table's name; drop them so SCHEMA_SQL recreates them on the new one.
    for index in [
        "idx_cash_movements_session_key",
        "idx_cash_movements_terminal_id",
        "idx_cash_movements_movement_type",
        "idx_cash_movements_created_at",
        "idx_cash_movements_sync",
    ] {
        tx.execute(format!("DROP INDEX IF EXISTS {index}").as_str())
            .await?;
    }
    let insert = format!(
        "INSERT INTO cash_movements (
            key, id, movement_number, movement_type, direction, amount_cents, session_key,
            terminal_id, category_key, payee_key, payee_name, note, recorded_by_user_id,
            recorded_by_name_snapshot, approved_by_user_id, approved_by_name_snapshot,
            recorded_late, original_variance_session_key, version, created_at, updated_at,
            deleted_at, updated_by_device
         )
         SELECT key, id, movement_number,
            CASE movement_type WHEN 'pay_in' THEN 'add_cash' WHEN 'pay_out_expense' THEN 'petty_expense'
                 WHEN 'pay_out_supplier' THEN 'supplier_payment' WHEN 'drawer_open_no_sale' THEN 'no_sale'
                 ELSE movement_type END,
            CASE movement_type WHEN 'pay_in' THEN 'in' WHEN 'cash_drop' THEN 'neutral'
                 WHEN 'drawer_open_no_sale' THEN 'neutral' ELSE 'out' END,
            amount_cents, session_key, terminal_id,
            CASE movement_type WHEN 'pay_in' THEN '{add_cash}' WHEN 'pay_out_expense' THEN '{petty}'
                 WHEN 'pay_out_supplier' THEN '{supplier}' WHEN 'drawer_open_no_sale' THEN '{no_sale}'
                 WHEN 'cash_drop' THEN '{drop}' ELSE '{owner}' END,
            supplier_key, recipient_or_source, reason, recorded_by_user_id,
            recorded_by_name_snapshot, approved_by_user_id, approved_by_name_snapshot,
            recorded_late, original_variance_session_key, version, created_at, updated_at,
            deleted_at, updated_by_device
         FROM cash_movements_old",
        add_cash = other(T::AddCash),
        petty = other(T::PettyExpense),
        supplier = other(T::SupplierPayment),
        no_sale = other(T::NoSale),
        drop = category_key(T::CashDrop, "To safe"),
        owner = other(T::OwnerWithdrawal),
    );
    tx.execute(insert.as_str()).await?;
    tx.execute("DROP TABLE cash_movements_old").await?;
    tx.commit().await?;
    Ok(())
}

/// Gives every purchase made before supplier receipts a receipt to belong to. Purchases that
/// share a supplier, an invoice number and a day become one receipt; a purchase without an
/// invoice number is a receipt of its own. Receipt keys and numbers are derived from the
/// smallest purchase key in the group, so every desktop that migrates the same history produces
/// the same rows and sync merges them instead of duplicating them. Runs after `SCHEMA_SQL`
/// (which creates `purchase_receipts`); the sync triggers are dropped for the backfill so it is
/// not uploaded, and `install_triggers` puts them back right after.
pub async fn migrate_purchases_to_receipts(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let columns = table_columns(pool, "purchases").await?;
    let has_old_columns = columns.iter().any(|c| c == "reference_no");
    let unmigrated: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM purchases WHERE receipt_key IS NULL")
            .fetch_one(pool)
            .await?;
    if unmigrated == 0 {
        return Ok(());
    }
    tracing::info!("Migrating {unmigrated} purchases into supplier receipts");

    // A database from before receipts has the invoice number and notes on the purchase row.
    let (reference, notes) = if has_old_columns {
        ("NULLIF(TRIM(reference_no), '')", "NULLIF(TRIM(notes), '')")
    } else {
        ("NULL", "NULL")
    };
    let group = format!("supplier_key, COALESCE({reference}, key), date");

    let mut tx = pool.begin().await?;
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_all(&mut *tx)
    .await?;
    for trigger in triggers {
        tx.execute(format!("DROP TRIGGER IF EXISTS \"{trigger}\"").as_str())
            .await?;
    }

    tx.execute(
        format!(
            "INSERT OR IGNORE INTO purchase_receipts (
                key, id, receipt_number, supplier_key, reference_no, date, notes,
                version, created_at, updated_at, updated_by_device
             )
             SELECT 'prc_leg_' || MIN(key), substr(hex(MIN(key)), -24),
                    'PUR-L' || substr(MIN(key), -10), supplier_key, MIN({reference}), date,
                    MIN({notes}), 1, MIN(created_at), MIN(created_at), NULL
             FROM purchases WHERE receipt_key IS NULL
             GROUP BY {group}"
        )
        .as_str(),
    )
    .await?;
    tx.execute(
        format!(
            "UPDATE purchases SET receipt_key = (
                SELECT 'prc_leg_' || MIN(p.key) FROM purchases p
                WHERE p.supplier_key = purchases.supplier_key AND p.date = purchases.date
                  AND COALESCE({}, p.key) = COALESCE({}, purchases.key)
             ) WHERE receipt_key IS NULL",
            reference.replace("reference_no", "p.reference_no"),
            reference.replace("reference_no", "purchases.reference_no"),
        )
        .as_str(),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// One-time move from per-product prices to stock batches, run on a database
/// whose `products` still carry `cost_price_cents`/`selling_price_cents`:
/// every product gets an opening batch (`stb_open_<productKey>`, same id and
/// timestamps as the product) holding its current stock at its last cost and
/// price; existing movements, serials and sale lines are pointed at it;
/// purchases get the price their units sold at; then the product price
/// columns are dropped. Every row it writes is derived from the product
/// alone, so a desktop and the cloud migrating separately end up with
/// identical rows that sync merges instead of duplicating. The capture
/// triggers are dropped first (`init_db` reinstalls them right after) so the
/// migration itself is not queued for upload, and the whole move is one
/// transaction. Waits for `stock_batches` to exist, i.e. for the pass after
/// `SCHEMA_SQL`.
pub async fn migrate_to_stock_batches(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let product_columns = table_columns(pool, "products").await?;
    if !product_columns.iter().any(|c| c == "cost_price_cents")
        || table_columns(pool, "stock_batches").await?.is_empty()
    {
        return Ok(());
    }
    tracing::info!("Migrating products to stock batches");

    for (table, column, ddl) in [
        (
            "stock_movements",
            "batch_key",
            "ALTER TABLE stock_movements ADD COLUMN batch_key TEXT",
        ),
        // NOT NULL needs a value for existing rows; every one is rewritten below.
        (
            "product_serials",
            "batch_key",
            "ALTER TABLE product_serials ADD COLUMN batch_key TEXT NOT NULL DEFAULT ''",
        ),
        (
            "purchases",
            "selling_price_cents",
            "ALTER TABLE purchases ADD COLUMN selling_price_cents INTEGER NOT NULL DEFAULT 0",
        ),
    ] {
        if !table_columns(pool, table)
            .await?
            .iter()
            .any(|c| c == column)
        {
            pool.execute(ddl).await?;
        }
    }

    let mut tx = pool.begin().await?;
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'sync\\_%' ESCAPE '\\'",
    )
    .fetch_all(&mut *tx)
    .await?;
    for trigger in triggers {
        tx.execute(format!("DROP TRIGGER IF EXISTS \"{trigger}\"").as_str())
            .await?;
    }

    tx.execute(
        "INSERT OR IGNORE INTO stock_batches (
            key, id, product_key, source, purchase_key, supplier_key, received_quantity,
            remaining_quantity, unit_cost_cents, selling_price_cents, received_at, version,
            created_at, updated_at, deleted_at, updated_by_device
         )
         SELECT 'stb_open_' || key, id, key, 'opening', NULL, NULL, stock_quantity,
                stock_quantity, cost_price_cents, selling_price_cents, created_at, 1,
                created_at, created_at, deleted_at, NULL
         FROM products",
    )
    .await?;
    // Audit-only return movements (no quantity) stay without a batch.
    tx.execute(
        "UPDATE stock_movements
         SET batch_key = 'stb_open_' || (SELECT p.key FROM products p WHERE p.id = stock_movements.product_id)
         WHERE batch_key IS NULL AND quantity_delta != 0
           AND EXISTS (SELECT 1 FROM products p WHERE p.id = stock_movements.product_id)",
    )
    .await?;
    tx.execute(
        "UPDATE product_serials SET batch_key = 'stb_open_' || product_key WHERE batch_key = ''",
    )
    .await?;
    tx.execute(
        "UPDATE purchases SET selling_price_cents =
            (SELECT p.selling_price_cents FROM products p WHERE p.key = purchases.product_key)
         WHERE selling_price_cents = 0
           AND EXISTS (SELECT 1 FROM products p WHERE p.key = purchases.product_key)",
    )
    .await?;

    let costs: std::collections::HashMap<String, i64> =
        sqlx::query_as::<_, (String, i64)>("SELECT key, cost_price_cents FROM products")
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();
    let cost_of = |key: &str| costs.get(key).copied();
    for (table, columns) in [
        ("invoices", &["items"][..]),
        ("credit_notes", &["returned_items", "exchange_items"][..]),
    ] {
        if table_columns(pool, table).await?.is_empty() {
            continue;
        }
        for column in columns {
            let rows: Vec<(String, Option<String>)> =
                sqlx::query_as(&format!("SELECT key, {column} FROM {table}"))
                    .fetch_all(&mut *tx)
                    .await?;
            for (key, json) in rows {
                let Some(json) = json else {
                    continue;
                };
                let mut items = serde_json::from_str(&json).map_err(|err| {
                    sqlx::Error::Decode(
                        format!("{table}.{column} of {key} is not JSON: {err}").into(),
                    )
                })?;
                if crate::modules::inventory::service::stock_batch::backfill_line_allocations(
                    &mut items, cost_of,
                ) {
                    sqlx::query(&format!("UPDATE {table} SET {column} = ? WHERE key = ?"))
                        .bind(items.to_string())
                        .bind(&key)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }
    }

    tx.execute("ALTER TABLE products DROP COLUMN cost_price_cents")
        .await?;
    tx.execute("ALTER TABLE products DROP COLUMN selling_price_cents")
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Formats the current UTC timestamp as an ISO-8601/RFC-3339 string.
#[must_use]
pub fn now_utc_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Parses a stored RFC-3339 timestamp; a value that is not one means the row is corrupt.
pub fn parse_iso_datetime(s: &str) -> AppResult<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|err| AppError::internal(format!("stored timestamp `{s}` is invalid: {err}")))
}

/// A SQL `WHERE` condition (without the `WHERE` keyword) and the values to bind to its `?`
/// placeholders, in order.
#[derive(Debug)]
pub struct SqlFilter {
    pub clause: String,
    pub bindings: Vec<String>,
}

/// Column names of `table` (empty when the table does not exist).
pub(super) async fn table_columns(
    pool: &SqlitePool,
    table: &str,
) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .fetch_all(pool)
        .await
}

/// Converts a stored RFC-3339 string into `mongodb::bson::DateTime`.
pub fn to_bson_datetime(s: &str) -> AppResult<mongodb::bson::DateTime> {
    Ok(mongodb::bson::DateTime::from_chrono(parse_iso_datetime(s)?))
}
