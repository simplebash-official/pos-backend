use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::sync_local::{PendingItem, PendingQuery, PendingResponse};

use crate::modules::sync::resources;

use crate::modules::sync::state::pool_of;

use sqlx::Row;

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
