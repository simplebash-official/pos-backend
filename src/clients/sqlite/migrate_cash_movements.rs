use super::table_columns::table_columns;

use sqlx::{Executor, SqlitePool};

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
