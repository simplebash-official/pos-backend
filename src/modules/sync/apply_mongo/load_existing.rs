use super::existing_row::ExistingRow;
use super::num_i64::num_i64;
use crate::clients::db::Db;
use crate::core::error::AppResult;
use crate::core::sync_merge::RowMeta;

use crate::modules::sync::cloud_store::coll;

use crate::modules::sync::resources::ResourceSpec;

use mongodb::bson::{Bson, Document, doc};

fn is_deleted(doc: &Document) -> bool {
    doc.get("deleted_at")
        .is_some_and(|v| !matches!(v, Bson::Null))
}

/// Loads the row with `key` (including tombstones) with its merge metadata.
pub(crate) async fn load_existing(
    db: &Db,
    spec: &ResourceSpec,
    key: &str,
) -> AppResult<Option<ExistingRow>> {
    let Some(doc) = coll(db, spec.table)?.find_one(doc! { "key": key }).await? else {
        return Ok(None);
    };
    let updated_at_ms = doc
        .get_datetime("updated_at")
        .map(|d| d.timestamp_millis())
        .unwrap_or(0);
    let meta = RowMeta {
        updated_at_ms,
        // Web-originated writes carry no device: they sort below any device id
        // at an identical millisecond.
        device_id: doc.get_str("updated_by_device").unwrap_or("").to_string(),
        version: num_i64(&doc, "version").unwrap_or(1),
        deleted: is_deleted(&doc),
    };
    Ok(Some(ExistingRow { meta, doc }))
}
