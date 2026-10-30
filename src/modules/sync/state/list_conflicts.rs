use super::pool_of::pool_of;
use crate::clients::db::Db;

use crate::core::error::AppResult;
use crate::domain::sync_local::ConflictItem;

use chrono::{DateTime, Utc};
use sqlx::Row;

pub async fn list_conflicts(db: &Db) -> AppResult<Vec<ConflictItem>> {
    let pool = pool_of(db)?;
    let rows = sqlx::query(
        "SELECT key, kind, resource, entity_key, detail, detected_at, resolved_at, resolution \
         FROM sync_conflicts WHERE resolved_at IS NULL ORDER BY detected_at DESC, key",
    )
    .fetch_all(pool)
    .await?;
    let parse = |s: String| -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(&s)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now())
    };
    Ok(rows
        .into_iter()
        .map(|row| ConflictItem {
            key: row.get("key"),
            kind: row.get("kind"),
            resource: row.get("resource"),
            entity_key: row.get("entity_key"),
            detail: serde_json::from_str(&row.get::<String, _>("detail")).unwrap_or_default(),
            detected_at: parse(row.get("detected_at")),
            resolved_at: row.get::<Option<String>, _>("resolved_at").map(parse),
            resolution: row.get("resolution"),
        })
        .collect())
}
