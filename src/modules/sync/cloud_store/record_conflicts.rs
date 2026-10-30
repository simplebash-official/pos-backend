use super::coll::coll;
use super::conflicts::CONFLICTS;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::domain::sync_v2::NewConflict;
use mongodb::bson;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Persists conflicts raised while applying a batch.
pub(crate) async fn record_conflicts(
    db: &Db,
    device_id: &str,
    conflicts: &[NewConflict],
) -> AppResult<()> {
    if conflicts.is_empty() {
        return Ok(());
    }
    let collection = coll(db, CONFLICTS)?;
    for conflict in conflicts {
        let detail = bson::serialize_to_bson(&conflict.detail)
            .map_err(|e| AppError::internal(format!("cannot store conflict detail: {e}")))?;
        collection
            .insert_one(doc! {
                "key": generate_id("scf"),
                "kind": conflict.kind.as_str(),
                "resource": &conflict.resource,
                "entity_key": &conflict.entity_key,
                "detail": detail,
                "device_id": device_id,
                "detected_at": BsonDateTime::now(),
            })
            .await?;
    }
    Ok(())
}
