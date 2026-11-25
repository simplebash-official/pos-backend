//! The device side of the catch-up (SQLite).

use super::ledger_resources::ledger_specs;
use crate::modules::sync::state;
use chrono::Utc;
use sqlx::{Executor, SqlitePool};

/// Queues, once, every ledger row of a device that is syncing: earlier builds could not upload
/// them, and an acknowledgement for the rows around one deleted it unsent. Rows the cloud already
/// has merge there as repeats. The `sync_state.ledger_requeued_at` column marks the work as done;
/// a new database has it from the schema.
pub async fn migrate_sqlite(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    // Reading the table comes first: it makes this connection load a schema change made on another
    // one, which the pragma function alone does not.
    let capturing: Option<bool> =
        sqlx::query_scalar("SELECT capture_enabled FROM sync_state WHERE id = 1")
            .fetch_optional(&mut *tx)
            .await?;
    let done: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('sync_state') \
         WHERE name = 'ledger_requeued_at')",
    )
    .fetch_one(&mut *tx)
    .await?;
    if done {
        return Ok(());
    }
    tx.execute("ALTER TABLE sync_state ADD COLUMN ledger_requeued_at TEXT")
        .await?;
    // A device that never enabled sync uploads everything when it does.
    if capturing == Some(true) {
        let mut queued = 0;
        for spec in ledger_specs() {
            queued += state::enqueue_every_row(&mut tx, spec).await?;
        }
        tracing::info!(queued, "queued the ledger rows for upload again");
    }
    sqlx::query("UPDATE sync_state SET ledger_requeued_at = ? WHERE id = 1")
        .bind(Utc::now().to_rfc3339())
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}
