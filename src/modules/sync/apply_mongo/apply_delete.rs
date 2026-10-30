use super::existing_row::ExistingRow;
use super::millis::millis;
use super::note_scope::note_scope;
use super::write_outcome::WriteOutcome;
use crate::clients::db::Db;
use crate::core::error::AppResult;

use crate::domain::sync_v2::{ChangeRecord, DerivedScope};
use crate::modules::sync::cloud_store::coll;

use crate::modules::sync::resources::ResourceSpec;

use mongodb::bson::doc;

/// Tombstones a row (soft delete). Deleting an unknown or already-deleted row
/// is a no-op.
pub(crate) async fn apply_delete(
    db: &Db,
    spec: &ResourceSpec,
    record: &ChangeRecord,
    effective_ms: i64,
    device_id: &str,
    existing: Option<&ExistingRow>,
    scope: &mut DerivedScope,
) -> AppResult<WriteOutcome> {
    let Some(existing) = existing else {
        return Ok(WriteOutcome::Unchanged);
    };
    if existing.meta.deleted {
        return Ok(WriteOutcome::Unchanged);
    }
    coll(db, spec.table)?
        .update_one(
            doc! { "key": &record.key },
            doc! { "$set": {
                "deleted_at": millis(effective_ms),
                "updated_at": millis(effective_ms),
                "updated_by_device": device_id,
                "version": existing.meta.version.max(record.version),
            }},
        )
        .await?;
    note_scope(spec, None, &record.key, &existing.doc, scope);
    Ok(WriteOutcome::Written)
}
