use super::canonical_name::canonical_name;
use crate::core::error::{AppError, AppResult};
use crate::domain::sync_local::StoreBlockRequest;
use crate::modules::sync::state::pool_of;

/// Stores a block the cloud reserved for this device.
pub async fn store_block(db: &crate::clients::db::Db, req: StoreBlockRequest) -> AppResult<()> {
    crate::core::logging::domain::tracked("sync.block_stored", async move {
        let pool = pool_of(db)?;
        if req.end < req.start || req.padding < 0 {
            return Err(AppError::validation("Invalid number block range"));
        }
        sqlx::query(
            "INSERT OR REPLACE INTO sync_number_blocks (name, prefix, padding, start_seq, end_seq, next_seq, expires_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(canonical_name(&req.name))
        .bind(&req.prefix)
        .bind(req.padding)
        .bind(req.start)
        .bind(req.end)
        .bind(req.start)
        .bind(req.expires_at.to_rfc3339())
        .execute(pool)
        .await?;
        Ok(())
    })
    .await
}
