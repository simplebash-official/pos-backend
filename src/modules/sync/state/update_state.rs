use super::get_state::get_state;
use super::pool_of::pool_of;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::sync_local::{SyncStateResponse, UpdateSyncStateRequest};

pub async fn update_state(db: &Db, req: UpdateSyncStateRequest) -> AppResult<SyncStateResponse> {
    crate::core::logging::domain::tracked("sync.state_updated", async move {
        let pool = pool_of(db)?;
        let mut tx = pool.begin().await?;
        if let Some(offset) = req.clock_offset_ms {
            sqlx::query("UPDATE sync_state SET clock_offset_ms = ? WHERE id = 1")
                .bind(offset)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(linked) = req.linked {
            sqlx::query("UPDATE sync_state SET linked = ? WHERE id = 1")
                .bind(linked as i64)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(tenant_id) = &req.tenant_id {
            sqlx::query("UPDATE sync_state SET tenant_id = ? WHERE id = 1")
                .bind(tenant_id)
                .execute(&mut *tx)
                .await?;
        }
        if let Some(cursor) = req.cloud_cursor {
            sqlx::query("UPDATE sync_state SET cloud_cursor = ? WHERE id = 1")
                .bind(cursor)
                .execute(&mut *tx)
                .await?;
        }
        if req.bootstrap_reset == Some(true) {
            sqlx::query("UPDATE sync_state SET bootstrap_active = 0 WHERE id = 1")
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        get_state(db).await
    })
    .await
}
