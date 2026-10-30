use super::table_columns::table_columns;

use sqlx::{Executor, SqlitePool};

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
