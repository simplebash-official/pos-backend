use super::pool_of::pool_of;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use chrono::Utc;

pub async fn resolve_conflict(db: &Db, key: &str, resolution: &str) -> AppResult<()> {
    crate::core::logging::domain::tracked("sync.conflict_resolved", async move {
        let pool = pool_of(db)?;
        let done = sqlx::query(
            "UPDATE sync_conflicts SET resolved_at = ?, resolution = ? WHERE key = ? AND resolved_at IS NULL",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(resolution)
        .bind(key)
        .execute(pool)
        .await?;
        if done.rows_affected() == 0 {
            return Err(AppError::not_found("Conflict not found or already resolved"));
        }
        Ok(())
    })
    .await
}
