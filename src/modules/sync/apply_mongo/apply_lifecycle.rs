use super::build_document::build_document;
use super::existing_row::ExistingRow;
use super::note_scope::note_scope;
use super::write_outcome::WriteOutcome;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::core::sync_merge::merge_lifecycle;
use crate::domain::sync_v2::{ChangeRecord, DerivedScope};
use crate::modules::sync::cloud_store::coll;
use crate::modules::sync::mongo_codec;

use crate::modules::sync::resources::ResourceSpec;
use crate::modules::sync::service::hydrate;

use mongodb::bson::doc;

/// Applies the lifecycle merge for an existing invoice / credit note: only its
/// lifecycle columns can change, the winning status ranked monotonically.
pub(crate) async fn apply_lifecycle(
    db: &Db,
    spec: &ResourceSpec,
    record: &ChangeRecord,
    effective_ms: i64,
    device_id: &str,
    existing: &ExistingRow,
    scope: &mut DerivedScope,
) -> AppResult<WriteOutcome> {
    let incoming = record
        .payload
        .as_ref()
        .ok_or_else(|| AppError::validation("upsert without payload"))?;
    let local = hydrate(db, spec.name, vec![existing.doc.clone()])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::internal("existing row could not be hydrated"))?;

    let merged = merge_lifecycle(spec.name, &local, incoming);
    if let Some(reason) = merged.rejected {
        return Ok(WriteOutcome::Rejected(reason));
    }
    if !merged.changed {
        return Ok(WriteOutcome::Unchanged);
    }

    let encoded = mongo_codec::encode(spec, &merged.merged)?;
    let winning_ms = effective_ms.max(existing.meta.updated_at_ms);
    let mut merged_record = record.clone();
    merged_record.payload = Some(merged.merged.clone());
    let document = build_document(
        spec,
        &merged_record,
        encoded,
        Some(existing),
        winning_ms,
        device_id,
    );
    coll(db, spec.table)?
        .replace_one(doc! { "key": &record.key }, document.clone())
        .await?;
    note_scope(spec, Some(&merged.merged), &record.key, &document, scope);
    Ok(WriteOutcome::Written)
}
