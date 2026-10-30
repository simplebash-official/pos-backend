use crate::clients::db::Db;
use crate::core::error::AppResult;

use crate::modules::sync::cloud_store;
use crate::modules::sync::cloud_store::{CHANGES, coll};
use chrono::{Duration, Utc};
use futures_util::TryStreamExt;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Compacts one tenant's change log (runs inside that tenant's scope).
/// Returns how many changes were dropped.
pub async fn compact_tenant(db: &Db, retention_days: i64) -> AppResult<u64> {
    let cutoff =
        BsonDateTime::from_millis((Utc::now() - Duration::days(retention_days)).timestamp_millis());
    let changes = coll(db, CHANGES)?;

    let mut newest = changes
        .find(doc! { "received_at": { "$lt": cutoff } })
        .sort(doc! { "seq": -1 })
        .limit(1)
        .await?;
    let newest_old = newest.try_next().await?.and_then(|d| d.get_i64("seq").ok());
    let Some(max_seq) = newest_old else {
        return Ok(0);
    };

    // Raise the floor BEFORE deleting, so a puller can never observe a
    // half-compacted range as if it were complete history.
    cloud_store::set_compacted_through(db, max_seq).await?;
    let deleted = changes
        .delete_many(doc! { "seq": { "$lte": max_seq } })
        .await?
        .deleted_count;
    Ok(deleted)
}
