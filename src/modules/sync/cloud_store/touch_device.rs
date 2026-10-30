use super::coll::coll;
use super::devices::DEVICES;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::{DateTime as BsonDateTime, Document, doc};

pub(crate) async fn touch_device(
    db: &Db,
    device_id: &str,
    outbox_seq: Option<i64>,
    batch_id: Option<&str>,
    cloud_cursor: Option<i64>,
) -> AppResult<()> {
    let mut set = doc! { "last_seen": BsonDateTime::now() };
    if let Some(batch_id) = batch_id {
        set.insert("last_batch_id", batch_id);
    }
    let mut update = doc! { "$set": set };
    let mut max = Document::new();
    if let Some(seq) = outbox_seq {
        max.insert("last_outbox_seq", seq);
    }
    if let Some(cursor) = cloud_cursor {
        max.insert("cloud_cursor", cursor);
    }
    if !max.is_empty() {
        update.insert("$max", max);
    }
    coll(db, DEVICES)?
        .update_one(doc! { "device_id": device_id }, update)
        .upsert(true)
        .await?;
    Ok(())
}
