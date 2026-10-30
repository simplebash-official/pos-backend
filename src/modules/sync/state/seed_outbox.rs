use super::pool_of::pool_of;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::sync_local::SeedResponse;

use crate::modules::sync::resources::{Phase, SYNC_RESOURCES};

/// Enqueues every row of every synced table (and records its merge metadata)
/// - the initial upload of a device that becomes a tenant's first device.
pub async fn seed_outbox(db: &Db) -> AppResult<SeedResponse> {
    let pool = pool_of(db)?;
    let mut tx = pool.begin().await?;
    let mut enqueued = 0i64;
    for spec in SYNC_RESOURCES.iter().filter(|s| s.phase == Phase::P3a) {
        let op_expr = if spec.soft_delete {
            "CASE WHEN deleted_at IS NOT NULL THEN 'delete' ELSE 'upsert' END"
        } else {
            "'upsert'"
        };
        let version_expr = if spec.table == "product_serials" {
            "1"
        } else {
            "version"
        };
        let inserted = sqlx::query(&format!(
            "INSERT OR REPLACE INTO sync_outbox (resource, key, op, enqueued_at) \
             SELECT '{name}', key, {op_expr}, strftime('%Y-%m-%dT%H:%M:%fZ', 'now') FROM {table}",
            name = spec.name,
            table = spec.table,
        ))
        .execute(&mut *tx)
        .await?
        .rows_affected();
        enqueued += inserted as i64;
        sqlx::query(&format!(
            "INSERT OR REPLACE INTO sync_row_meta (resource, key, updated_at_ms, device_id, version) \
             SELECT '{name}', key, \
                    CAST(ROUND((julianday(updated_at) - 2440587.5) * 86400000.0) AS INTEGER) \
                      + (SELECT clock_offset_ms FROM sync_state WHERE id = 1), \
                    (SELECT device_id FROM sync_state WHERE id = 1), {version_expr} FROM {table}",
            name = spec.name,
            table = spec.table,
        ))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(SeedResponse { enqueued })
}
