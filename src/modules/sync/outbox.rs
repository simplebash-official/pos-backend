// Reads the device outbox as canonical change records: each queued row is
// resolved to its CURRENT state (the outbox only remembers that something
// changed), hydrated to the same DTO the REST endpoints return, and stamped
// with the merge metadata the triggers recorded (skew-corrected, monotonic
// time and the writing device).

use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::{
    clients::{db::Db, sqlite::map_sqlite_row_to_document},
    core::error::AppResult,
    domain::{
        sync_local::{
            OutboxItem, OutboxQuery, OutboxResponse, PendingItem, PendingQuery, PendingResponse,
        },
        sync_v2::{ChangeOp, ChangeRecord},
    },
    modules::sync::{resources, service::hydrate, state::pool_of},
};

const DEFAULT_LIMIT: i64 = 200;
const MAX_LIMIT: i64 = 500;

pub(crate) fn ms_to_datetime(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap_or_else(Utc::now)
}

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

    Ok(OutboxResponse { items, last_seq })
}

const PENDING_DEFAULT_LIMIT: i64 = 50;
const PENDING_MAX_LIMIT: i64 = 200;

/// What is still waiting to upload, newest change first, with a readable name
/// for each row. Read-only: nothing is dequeued or hydrated.
pub async fn list_pending(db: &Db, query: PendingQuery) -> AppResult<PendingResponse> {
    let pool = pool_of(db)?;
    let limit = query
        .limit
        .unwrap_or(PENDING_DEFAULT_LIMIT)
        .clamp(1, PENDING_MAX_LIMIT);
    // Rows may be stored under the wire or the table name; match both.
    let wanted = query
        .resource
        .as_deref()
        .and_then(resources::spec)
        .map(|s| (s.name, s.table));

    let (where_sql, total_sql) = if wanted.is_some() {
        (
            " WHERE resource IN (?, ?)",
            "SELECT COUNT(*) FROM sync_outbox WHERE resource IN (?, ?)",
        )
    } else {
        ("", "SELECT COUNT(*) FROM sync_outbox")
    };
    let total = match wanted {
        Some((a, b)) => sqlx::query_scalar(total_sql).bind(a).bind(b),
        None => sqlx::query_scalar(total_sql),
    }
    .fetch_one(pool)
    .await?;
    let list_sql = format!(
        "SELECT resource, key, op, enqueued_at FROM sync_outbox{where_sql} ORDER BY seq DESC LIMIT ?"
    );
    let rows = match wanted {
        Some((a, b)) => sqlx::query(&list_sql).bind(a).bind(b).bind(limit),
        None => sqlx::query(&list_sql).bind(limit),
    }
    .fetch_all(pool)
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for r in rows {
        let raw: String = r.get("resource");
        let key: String = r.get("key");
        let Some(spec) = resources::spec(&raw) else {
            continue;
        };
        let label = match resources::label_column(spec.table) {
            Some(col) => {
                // `col` and `spec.table` come from static tables, never from the request.
                let sql = format!("SELECT {col} FROM {} WHERE key = ?", spec.table);
                sqlx::query_scalar::<_, Option<String>>(&sql)
                    .bind(&key)
                    .fetch_optional(pool)
                    .await?
                    .flatten()
                    .filter(|v| !v.trim().is_empty())
            }
            None => None,
        };
        items.push(PendingItem {
            resource: spec.name.to_string(),
            key,
            op: r.get("op"),
            enqueued_at: r.get("enqueued_at"),
            label,
        });
    }
    Ok(PendingResponse { items, total })
}
