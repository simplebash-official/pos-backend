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
    if !user_cols.is_empty() && !user_cols.iter().any(|(_, n)| n == "pin_hash") {
        tracing::info!("Migrating 'users' table: adding 'pin_hash' column");
        let _ = pool
            .execute("ALTER TABLE users ADD COLUMN pin_hash TEXT")
            .await;
    }

    let invoice_cols = table_columns(pool, "invoices").await.unwrap_or_default();
    if !invoice_cols.is_empty() && !invoice_cols.iter().any(|n| n == "session_key") {
        tracing::info!("Migrating 'invoices' table: adding 'session_key' column");
        let _ = pool
            .execute("ALTER TABLE invoices ADD COLUMN session_key TEXT")
            .await;
        let _ = pool
            .execute("CREATE INDEX IF NOT EXISTS idx_invoices_session_key ON invoices(session_key)")
            .await;
    }

    let payment_cols = table_columns(pool, "payments").await.unwrap_or_default();
    if !payment_cols.is_empty() && !payment_cols.iter().any(|n| n == "session_key") {
        tracing::info!("Migrating 'payments' table: adding 'session_key' column");
        let _ = pool
            .execute("ALTER TABLE payments ADD COLUMN session_key TEXT")
            .await;
        let _ = pool
            .execute("CREATE INDEX IF NOT EXISTS idx_payments_session_key ON payments(session_key)")
            .await;
    }

    let credit_note_cols = table_columns(pool, "credit_notes").await.unwrap_or_default();
    if !credit_note_cols.is_empty() && !credit_note_cols.iter().any(|n| n == "session_key") {
        tracing::info!("Migrating 'credit_notes' table: adding 'session_key' column");
        let _ = pool
            .execute("ALTER TABLE credit_notes ADD COLUMN session_key TEXT")
            .await;
        let _ = pool
            .execute("CREATE INDEX IF NOT EXISTS idx_credit_notes_session_key ON credit_notes(session_key)")
            .await;
    }

    let shop_profile_cols = table_columns(pool, "shop_profiles").await.unwrap_or_default();
    if !shop_profile_cols.is_empty() && !shop_profile_cols.iter().any(|n| n == "allow_shared_user_sessions") {
        tracing::info!("Migrating 'shop_profiles' table: adding 'allow_shared_user_sessions' column");
        let _ = pool
            .execute("ALTER TABLE shop_profiles ADD COLUMN allow_shared_user_sessions INTEGER NOT NULL DEFAULT 0")
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
        // Which design rendered the PDF (nullable: rows from before designs
        // were recorded keep NULL).
        for column in ["template_ref TEXT", "template_title TEXT", "template_version INTEGER"] {
            let name = column.split(' ').next().expect("column name");
            if !gen_doc_cols.iter().any(|(_, existing)| existing == name) {
                tracing::info!("Migrating 'generated_documents' table: adding '{name}' column");
                pool.execute(format!("ALTER TABLE generated_documents ADD COLUMN {column}").as_str())
                    .await?;
            }
        }
        let _ = pool
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_generated_documents_lookup ON generated_documents(entity_key, document_type, created_at DESC)",
            )
            .await;
    }

    // Per-design option values a shop saved (`family -> key -> value`).
    let settings_cols = table_columns(pool, "document_template_settings")
        .await
        .unwrap_or_default();
    if !settings_cols.is_empty() && !settings_cols.iter().any(|n| n == "options") {
        tracing::info!("Migrating 'document_template_settings' table: adding 'options' column");
        pool.execute(
            "ALTER TABLE document_template_settings ADD COLUMN options TEXT NOT NULL DEFAULT '{}'",
        )
        .await?;
    }

    // `quotations` was first created without the sync columns. The capture triggers
    // (`sync_capture::install_triggers`) read `NEW.version`, so every write to the
    // table would fail if these were missing: unlike the best-effort ALTERs above,
    // an error here is propagated.
    let quotation_cols: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('quotations')")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    if !quotation_cols.is_empty() {
        for (column, ddl) in [
            (
                "version",
                "ALTER TABLE quotations ADD COLUMN version INTEGER NOT NULL DEFAULT 1",
            ),
            (
                "deleted_at",
                "ALTER TABLE quotations ADD COLUMN deleted_at TEXT",
            ),
            (
                "updated_by_device",
                "ALTER TABLE quotations ADD COLUMN updated_by_device TEXT",
            ),
        ] {
            if !quotation_cols.iter().any(|(_, name)| name == column) {
                tracing::info!("Migrating 'quotations' table: adding '{column}' column");
                pool.execute(ddl).await?;
            }
        }
    }

    migrate_to_stock_batches(pool).await?;
    migrate_cash_movements(pool).await?;
    // Reversals and manager overrides arrived after typed movements did.
    let movement_columns = table_columns(pool, "cash_movements").await?;
    if !movement_columns.is_empty() {
        for (column, ddl) in [
            ("approval_reason", "ALTER TABLE cash_movements ADD COLUMN approval_reason TEXT"),
            ("reverses_key", "ALTER TABLE cash_movements ADD COLUMN reverses_key TEXT"),
        ] {
            if !movement_columns.iter().any(|c| c == column) {
                pool.execute(ddl).await?;
            }
        }
    }

    Ok(())
}

/// Column names of `table` (empty when the table does not exist).
async fn table_columns(pool: &SqlitePool, table: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .fetch_all(pool)
        .await
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
    use crate::domain::{cash_categories::category_key, sessions::CashMovementType as T};

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
                let Some(mut items) = json.and_then(|j| serde_json::from_str(&j).ok()) else {
                    continue;
                };
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
