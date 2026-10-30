use super::pool_of::pool_of;
use crate::clients::db::Db;

use crate::core::error::AppResult;

/// Drops outbox rows the cloud has accepted (`seq <= up_to`). Rows enqueued
/// while a push was in flight have higher sequence numbers and are kept.
pub async fn ack_outbox(db: &Db, up_to_seq: i64) -> AppResult<i64> {
    crate::core::logging::domain::tracked("sync.outbox_acked", async move {
        let pool = pool_of(db)?;
        let mut tx = pool.begin().await?;
        let removed = sqlx::query("DELETE FROM sync_outbox WHERE seq <= ?")
            .bind(up_to_seq)
            .execute(&mut *tx)
            .await?
            .rows_affected() as i64;
        sqlx::query(
            "UPDATE sync_state SET last_pushed_outbox_seq = MAX(last_pushed_outbox_seq, ?) WHERE id = 1",
        )
        .bind(up_to_seq)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(removed)
    })
    .await
}
