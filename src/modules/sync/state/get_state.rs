use super::pool_of::pool_of;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::sync_local::{NumberBlockInfo, ResourceCount, SyncStateResponse};
use crate::modules::sync::resources;

use chrono::Utc;
use sqlx::{Row, SqlitePool};

pub async fn get_state(db: &Db) -> AppResult<SyncStateResponse> {
    let pool = pool_of(db)?;
    let row = sqlx::query(
        "SELECT device_id, tenant_id, linked, capture_enabled, cloud_cursor, last_pushed_outbox_seq, \
         clock_offset_ms, bootstrap_active FROM sync_state WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;
    let pending_out: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_outbox")
        .fetch_one(pool)
        .await?;
    let conflicts_open: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_conflicts WHERE resolved_at IS NULL")
            .fetch_one(pool)
            .await?;
    let local_has_data: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM invoices)")
        .fetch_one(pool)
        .await?;
    let block_rows = sqlx::query(
        "SELECT name, \
                SUM(CASE WHEN end_seq >= next_seq THEN end_seq - next_seq + 1 ELSE 0 END) AS remaining, \
                MAX(end_seq - start_seq + 1) AS block_size \
         FROM sync_number_blocks WHERE expires_at > ? GROUP BY name ORDER BY name",
    )
    .bind(Utc::now().to_rfc3339())
    .fetch_all(pool)
    .await?;
    let number_blocks = block_rows
        .into_iter()
        .map(|r| NumberBlockInfo {
            name: r.get("name"),
            remaining: r.get("remaining"),
            block_size: r.get("block_size"),
        })
        .collect();
    let pending_by_resource = grouped_counts(pool, "sync_outbox", None).await?;
    let conflicts_by_resource =
        grouped_counts(pool, "sync_conflicts", Some("resolved_at IS NULL")).await?;
    Ok(SyncStateResponse {
        device_id: row.get("device_id"),
        tenant_id: row.get("tenant_id"),
        linked: row.get::<i64, _>("linked") != 0,
        capture_enabled: row.get::<i64, _>("capture_enabled") != 0,
        cloud_cursor: row.get("cloud_cursor"),
        last_pushed_outbox_seq: row.get("last_pushed_outbox_seq"),
        clock_offset_ms: row.get("clock_offset_ms"),
        bootstrap_active: row.get::<i64, _>("bootstrap_active") != 0,
        pending_out,
        conflicts_open,
        local_has_data,
        number_blocks,
        pending_by_resource,
        conflicts_by_resource,
    })
}

/// Row counts per `resource`, normalised to the wire name. `table` and
/// `filter` are fixed strings from this module, never user input.
async fn grouped_counts(
    pool: &SqlitePool,
    table: &str,
    filter: Option<&str>,
) -> AppResult<Vec<ResourceCount>> {
    let sql = format!(
        "SELECT resource, COUNT(*) AS n FROM {table}{} GROUP BY resource",
        filter.map(|f| format!(" WHERE {f}")).unwrap_or_default()
    );
    let mut merged: Vec<ResourceCount> = Vec::new();
    for r in sqlx::query(&sql).fetch_all(pool).await? {
        let raw: String = r.get("resource");
        let name = resources::spec(&raw)
            .map(|s| s.name)
            .unwrap_or(raw.as_str());
        let n: i64 = r.get("n");
        match merged.iter_mut().find(|c| c.resource == name) {
            Some(c) => c.count += n,
            None => merged.push(ResourceCount {
                resource: name.to_string(),
                count: n,
            }),
        }
    }
    merged.sort_by(|a, b| a.resource.cmp(&b.resource));
    Ok(merged)
}
