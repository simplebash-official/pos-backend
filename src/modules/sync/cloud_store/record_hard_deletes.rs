use super::tombstones::TOMBSTONES;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

/// Records that rows of `table` were removed outright (not soft-deleted), so
/// the change-stream consumer can publish them as deletes: a removed document
/// leaves nothing behind for it to read. A no-op outside the multi-tenant
/// cloud (no change log there) and for an empty `keys`.
pub(crate) async fn record_hard_deletes(db: &Db, table: &str, keys: &[String]) -> AppResult<()> {
    let Some(mongo) = db.as_mongo() else {
        return Ok(());
    };
    if keys.is_empty() || !mongo.effective_tenant().is_scoped() {
        return Ok(());
    }
    let origin = crate::core::sync_origin::current_origin()
        .map(|o| o.to_string())
        .unwrap_or_else(|| crate::core::sync_origin::CLOUD_ORIGIN.to_string());
    let tombstones = mongo.collection::<Document>(TOMBSTONES);
    let now = BsonDateTime::now();
    for key in keys {
        tombstones
            .insert_one(doc! {
                "table": table,
                "key": key,
                "deleted_at": now,
                "updated_by_device": &origin,
            })
            .await?;
    }
    Ok(())
}
