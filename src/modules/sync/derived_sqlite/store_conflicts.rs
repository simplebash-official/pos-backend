use super::now_iso::now_iso;
use crate::core::error::AppResult;
use crate::core::id::generate_id;

use crate::domain::sync_v2::NewConflict;

use sqlx::SqliteConnection;

/// Persists conflicts raised while applying or recomputing.
pub async fn store_conflicts(
    conn: &mut SqliteConnection,
    conflicts: &[NewConflict],
) -> AppResult<()> {
    for conflict in conflicts {
        sqlx::query(
            "INSERT INTO sync_conflicts (key, kind, resource, entity_key, detail, detected_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(generate_id("scf"))
        .bind(conflict.kind.as_str())
        .bind(&conflict.resource)
        .bind(&conflict.entity_key)
        .bind(serde_json::to_string(&conflict.detail)?)
        .bind(now_iso())
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
