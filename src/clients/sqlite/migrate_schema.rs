use super::clients_sqlite::derive_username;
use super::clients_sqlite::migrate_cash_movements;
use super::clients_sqlite::migrate_to_stock_batches;
use super::clients_sqlite::table_columns;

use sqlx::{Executor, SqlitePool};

/// Applies incremental migrations for legacy databases (e.g., removing deprecated columns).
pub async fn migrate_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    // `suppliers.contact_persons` arrived with multiple contact persons support.
    let supplier_cols = table_columns(pool, "suppliers").await?;
    if !supplier_cols.is_empty() && !supplier_cols.iter().any(|c| c == "contact_persons") {
        tracing::info!("Migrating 'suppliers' table: adding 'contact_persons' column");
        pool.execute("ALTER TABLE suppliers ADD COLUMN contact_persons TEXT NOT NULL DEFAULT '[]'")
            .await?;
        let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT key, contact_person, primary_phone, secondary_phone FROM suppliers WHERE contact_person != '' AND primary_phone != ''"
        )
        .fetch_all(pool)
        .await?;
        for (key, contact, primary, secondary) in rows {
            let mut phones = vec![primary];
            if let Some(sec) = secondary
                && !sec.trim().is_empty()
            {
                phones.push(sec);
            }
            let contact_persons = vec![serde_json::json!({
                "name": contact,
                "phoneNumbers": phones,
            })];
            let json =
                serde_json::to_string(&contact_persons).expect("contact persons serialise to JSON");
            sqlx::query("UPDATE suppliers SET contact_persons = ? WHERE key = ?")
                .bind(json)
                .bind(key)
                .execute(pool)
                .await?;
        }
    }
    // `cash_movements.receipt_key` arrived with supplier receipts too.
    let movement_cols = table_columns(pool, "cash_movements").await?;
    if !movement_cols.is_empty() && !movement_cols.iter().any(|c| c == "receipt_key") {
        tracing::info!("Migrating 'cash_movements' table: adding 'receipt_key' column");
        pool.execute("ALTER TABLE cash_movements ADD COLUMN receipt_key TEXT")
            .await?;
    }
    // `purchases.receipt_key` arrived with supplier receipts. Added nullable (SQLite cannot add
    // a NOT NULL column without a default); `migrate_purchases_to_receipts` fills it.
    let purchase_cols = table_columns(pool, "purchases").await?;
    if !purchase_cols.is_empty() && !purchase_cols.iter().any(|c| c == "receipt_key") {
        tracing::info!("Migrating 'purchases' table: adding 'receipt_key' column");
        pool.execute("ALTER TABLE purchases ADD COLUMN receipt_key TEXT")
            .await?;
    }
    // `receipt_charges.description` (what an "other" charge is) arrived after the table itself.
    let charge_cols = table_columns(pool, "receipt_charges").await?;
    if !charge_cols.is_empty() && !charge_cols.iter().any(|c| c == "description") {
        tracing::info!("Migrating 'receipt_charges' table: adding 'description' column");
        pool.execute("ALTER TABLE receipt_charges ADD COLUMN description TEXT")
            .await?;
    }
    // A purchase line is priced by its total (`line_total_cents`, the supplier's lot price) with
    // its share of the receipt's extra charges beside it; the per-unit `unit_cost_cents` is gone,
    // so the line has one source of truth. Existing lines get quantity x unit cost and no share.
    // The sync triggers name columns, so they are dropped for the change and `install_triggers`
    // puts them back right after; the rewrite is not uploaded.
    let purchase_cols = table_columns(pool, "purchases").await?;
    if purchase_cols.iter().any(|c| c == "unit_cost_cents") {
        tracing::info!("Migrating 'purchases' table: unit cost to line total");
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
        if !purchase_cols.iter().any(|c| c == "line_total_cents") {
            tx.execute(
                "ALTER TABLE purchases ADD COLUMN line_total_cents INTEGER NOT NULL DEFAULT 0",
            )
            .await?;
            tx.execute(
                "ALTER TABLE purchases ADD COLUMN charges_share_cents INTEGER NOT NULL DEFAULT 0",
            )
            .await?;
        }
        tx.execute("UPDATE purchases SET line_total_cents = quantity * unit_cost_cents")
            .await?;
        tx.execute("ALTER TABLE purchases DROP COLUMN unit_cost_cents")
            .await?;
        tx.commit().await?;
    }

    // Check if 'login_sessions' table has a legacy 'user_id' column from earlier revisions
    let columns: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('login_sessions')")
            .fetch_all(pool)
            .await?;

    if columns.iter().any(|(_, name)| name == "user_id") {
        tracing::info!("Migrating legacy column 'user_id' from 'login_sessions' table");
        pool.execute("ALTER TABLE login_sessions DROP COLUMN user_id")
            .await?;
    }

    // `sync_state.outbox_epoch` (see `sync_capture::ensure_state`) arrived after
    // the table did.
    let sync_state_cols: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('sync_state')")
            .fetch_all(pool)
            .await?;
    if !sync_state_cols.is_empty() && !sync_state_cols.iter().any(|(_, n)| n == "outbox_epoch") {
        pool.execute("ALTER TABLE sync_state ADD COLUMN outbox_epoch TEXT")
            .await?;
    }

    migrate_users_to_usernames(pool).await?;
    migrate_login_sessions_to_usernames(pool).await?;

    // `users.preferences_json` (per-login UI preferences) arrived after the table did.
    let user_cols: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('users')")
            .fetch_all(pool)
            .await?;
    if !user_cols.is_empty() && !user_cols.iter().any(|(_, n)| n == "preferences_json") {
        tracing::info!("Migrating 'users' table: adding 'preferences_json' column");
        pool.execute("ALTER TABLE users ADD COLUMN preferences_json TEXT")
            .await?;
    }
    if !user_cols.is_empty() && !user_cols.iter().any(|(_, n)| n == "pin_hash") {
        tracing::info!("Migrating 'users' table: adding 'pin_hash' column");
        pool.execute("ALTER TABLE users ADD COLUMN pin_hash TEXT")
            .await?;
    }

    let invoice_cols = table_columns(pool, "invoices").await?;
    if !invoice_cols.is_empty() && !invoice_cols.iter().any(|n| n == "session_key") {
        tracing::info!("Migrating 'invoices' table: adding 'session_key' column");
        pool.execute("ALTER TABLE invoices ADD COLUMN session_key TEXT")
            .await?;
        pool.execute(
            "CREATE INDEX IF NOT EXISTS idx_invoices_session_key ON invoices(session_key)",
        )
        .await?;
    }

    let payment_cols = table_columns(pool, "payments").await?;
    if !payment_cols.is_empty() && !payment_cols.iter().any(|n| n == "session_key") {
        tracing::info!("Migrating 'payments' table: adding 'session_key' column");
        pool.execute("ALTER TABLE payments ADD COLUMN session_key TEXT")
            .await?;
        pool.execute(
            "CREATE INDEX IF NOT EXISTS idx_payments_session_key ON payments(session_key)",
        )
        .await?;
    }

    let credit_note_cols = table_columns(pool, "credit_notes").await?;
    if !credit_note_cols.is_empty() && !credit_note_cols.iter().any(|n| n == "session_key") {
        tracing::info!("Migrating 'credit_notes' table: adding 'session_key' column");
        pool.execute("ALTER TABLE credit_notes ADD COLUMN session_key TEXT")
            .await?;
        pool.execute(
            "CREATE INDEX IF NOT EXISTS idx_credit_notes_session_key ON credit_notes(session_key)",
        )
        .await?;
    }

    let shop_profile_cols = table_columns(pool, "shop_profiles").await?;
    if !shop_profile_cols.is_empty()
        && !shop_profile_cols
            .iter()
            .any(|n| n == "allow_shared_user_sessions")
    {
        tracing::info!(
            "Migrating 'shop_profiles' table: adding 'allow_shared_user_sessions' column"
        );
        pool
            .execute("ALTER TABLE shop_profiles ADD COLUMN allow_shared_user_sessions INTEGER NOT NULL DEFAULT 0")
            .await?;
    }

    // Check if 'generated_documents' table is missing required columns from earlier schema versions
    let gen_doc_cols: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('generated_documents')")
            .fetch_all(pool)
            .await?;

    if !gen_doc_cols.is_empty() {
        if !gen_doc_cols.iter().any(|(_, name)| name == "entity_key") {
            tracing::info!(
                "Migrating 'generated_documents' table: adding missing 'entity_key' column"
            );
            pool.execute(
                "ALTER TABLE generated_documents ADD COLUMN entity_key TEXT NOT NULL DEFAULT ''",
            )
            .await?;
        }
        if !gen_doc_cols.iter().any(|(_, name)| name == "document_type") {
            tracing::info!(
                "Migrating 'generated_documents' table: adding missing 'document_type' column"
            );
            pool.execute(
                "ALTER TABLE generated_documents ADD COLUMN document_type TEXT NOT NULL DEFAULT ''",
            )
            .await?;
        }
        if !gen_doc_cols
            .iter()
            .any(|(_, name)| name == "file_size_bytes")
        {
            tracing::info!(
                "Migrating 'generated_documents' table: adding missing 'file_size_bytes' column"
            );
            pool
                .execute(
                    "ALTER TABLE generated_documents ADD COLUMN file_size_bytes INTEGER NOT NULL DEFAULT 0",
                )
                .await?;
        }
        // Which design rendered the PDF (nullable: rows from before designs
        // were recorded keep NULL).
        for column in [
            "template_ref TEXT",
            "template_title TEXT",
            "template_version INTEGER",
        ] {
            let name = column.split(' ').next().expect("column name");
            if !gen_doc_cols.iter().any(|(_, existing)| existing == name) {
                tracing::info!("Migrating 'generated_documents' table: adding '{name}' column");
                pool.execute(
                    format!("ALTER TABLE generated_documents ADD COLUMN {column}").as_str(),
                )
                .await?;
            }
        }
        pool
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_generated_documents_lookup ON generated_documents(entity_key, document_type, created_at DESC)",
            )
            .await?;
    }

    // Per-design option values a shop saved (`family -> key -> value`).
    let settings_cols = table_columns(pool, "document_template_settings").await?;
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
            .await?;
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
            (
                "fulfilled_by_invoice_key",
                "ALTER TABLE quotations ADD COLUMN fulfilled_by_invoice_key TEXT",
            ),
            (
                "fulfilled_at",
                "ALTER TABLE quotations ADD COLUMN fulfilled_at TEXT",
            ),
            (
                "has_out_of_stock_items",
                "ALTER TABLE quotations ADD COLUMN has_out_of_stock_items INTEGER NOT NULL DEFAULT 0",
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
            (
                "approval_reason",
                "ALTER TABLE cash_movements ADD COLUMN approval_reason TEXT",
            ),
            (
                "reverses_key",
                "ALTER TABLE cash_movements ADD COLUMN reverses_key TEXT",
            ),
        ] {
            if !movement_columns.iter().any(|c| c == column) {
                pool.execute(ddl).await?;
            }
        }
    }

    Ok(())
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
            .await?;
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
            .await?;
    if !session_cols.iter().any(|(_, n)| n == "email_at_login") {
        return Ok(());
    }
    sqlx::query("ALTER TABLE login_sessions RENAME COLUMN email_at_login TO username_at_login")
        .execute(pool)
        .await?;
    let user_cols: Vec<(i32, String)> =
        sqlx::query_as("SELECT cid, name FROM pragma_table_info('users')")
            .fetch_all(pool)
            .await?;
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
