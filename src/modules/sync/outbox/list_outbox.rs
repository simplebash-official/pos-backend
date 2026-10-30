use super::ms_to_datetime::ms_to_datetime;
use crate::clients::db::Db;
use crate::clients::sqlite::map_sqlite_row_to_document;
use crate::core::error::AppResult;
use crate::domain::sync_local::{OutboxItem, OutboxQuery, OutboxResponse};
use crate::domain::sync_v2::{ChangeOp, ChangeRecord};
use crate::modules::sync::resources;
use crate::modules::sync::service::hydrate;
use crate::modules::sync::state::pool_of;
use chrono::Utc;
use sqlx::Row;

const DEFAULT_LIMIT: i64 = 200;

const MAX_LIMIT: i64 = 500;

pub async fn list_outbox(db: &Db, query: OutboxQuery) -> AppResult<OutboxResponse> {
    let pool = pool_of(db)?;
    let after = query.after.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let queued = sqlx::query(
        "SELECT seq, resource, key, op FROM sync_outbox WHERE seq > ? ORDER BY seq ASC LIMIT ?",
    )
    .bind(after)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let local_device: String = sqlx::query_scalar("SELECT device_id FROM sync_state WHERE id = 1")
        .fetch_one(pool)
        .await?;

    let mut items = Vec::with_capacity(queued.len());
    let mut last_seq = after;
    for entry in queued {
        let seq: i64 = entry.get("seq");
        last_seq = seq;
        let resource: String = entry.get("resource");
        let key: String = entry.get("key");
        let queued_op: String = entry.get("op");
        let Some(spec) = resources::spec(&resource) else {
            continue;
        };

        let meta = sqlx::query(
            "SELECT updated_at_ms, device_id, version FROM sync_row_meta WHERE resource = ? AND key = ?",
        )
        .bind(spec.name)
        .bind(&key)
        .fetch_optional(pool)
        .await?;
        let (updated_at, device_id, meta_version) = match &meta {
            Some(m) => (
                ms_to_datetime(m.get("updated_at_ms")),
                m.get::<String, _>("device_id"),
                m.get::<i64, _>("version"),
            ),
            None => (Utc::now(), local_device.clone(), 1),
        };

        let row = sqlx::query(&format!("SELECT * FROM {} WHERE key = ?", spec.table))
            .bind(&key)
            .fetch_optional(pool)
            .await?;

        let tombstoned = match &row {
            None => true,
            Some(r) => spec.soft_delete && r.get::<Option<String>, _>("deleted_at").is_some(),
        };

        let record = if tombstoned || queued_op == "delete" {
            ChangeRecord {
                resource: spec.name.to_string(),
                key,
                op: ChangeOp::Delete,
                version: meta_version,
                updated_at,
                device_id,
                payload: None,
            }
        } else {
            let row = row.expect("checked above: row exists for an upsert");
            let legacy_id: String = row.get("id");
            let version: i64 = if spec.table == "product_serials" {
                meta_version
            } else {
                row.get("version")
            };
            let document = map_sqlite_row_to_document(&row);
            let mut hydrated = hydrate(db, spec.name, vec![document]).await?;
            let Some(mut payload) = hydrated.pop() else {
                continue;
            };
            // Some DTOs (supplier links) do not surface the legacy id, which
            // the receiver must preserve because foreign keys still use it.
            if let Some(object) = payload.as_object_mut() {
                object
                    .entry("id".to_string())
                    .or_insert(serde_json::Value::String(legacy_id));
            }
            ChangeRecord {
                resource: spec.name.to_string(),
                key,
                op: ChangeOp::Upsert,
                version,
                updated_at,
                device_id,
                payload: Some(payload),
            }
        };
        items.push(OutboxItem { seq, record });
    }

    let epoch: Option<String> =
        sqlx::query_scalar("SELECT outbox_epoch FROM sync_state WHERE id = 1")
            .fetch_optional(pool)
            .await?
            .flatten();
    Ok(OutboxResponse {
        items,
        last_seq,
        epoch: epoch.unwrap_or_default(),
    })
}
