use super::migrate_opening_balances::migrate_opening_balances;
use super::pool_of::pool_of;
use super::seed_outbox::seed_outbox;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::sync_local::{EnableRequest, EnableResponse};

/// Turns capture on: adopts the cloud-issued device identity, makes stock a pure
/// ledger, flips `capture_enabled` and `linked`, then enqueues every existing
/// row so the first push uploads the whole shop.
pub async fn enable(db: &Db, req: EnableRequest) -> AppResult<EnableResponse> {
    crate::core::logging::domain::tracked("sync.enabled", async move {
        let pool = pool_of(db)?;
        let opening_balances_created = migrate_opening_balances(pool).await?;
        if let Some(device_id) = req.device_id.as_deref().filter(|d| !d.is_empty()) {
            sqlx::query("UPDATE sync_state SET device_id = ? WHERE id = 1")
                .bind(device_id)
                .execute(pool)
                .await?;
        }
        if let Some(tenant_id) = req.tenant_id.as_deref().filter(|t| !t.is_empty()) {
            sqlx::query("UPDATE sync_state SET tenant_id = ? WHERE id = 1")
                .bind(tenant_id)
                .execute(pool)
                .await?;
        }
        sqlx::query("UPDATE sync_state SET capture_enabled = 1, linked = 1 WHERE id = 1")
            .execute(pool)
            .await?;
        let enqueued = seed_outbox(db).await?.enqueued;
        let device_id: String = sqlx::query_scalar("SELECT device_id FROM sync_state WHERE id = 1")
            .fetch_one(pool)
            .await?;
        Ok(EnableResponse {
            device_id,
            opening_balances_created,
            enqueued,
        })
    })
    .await
}
