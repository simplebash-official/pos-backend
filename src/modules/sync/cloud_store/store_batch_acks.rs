use super::batches::BATCHES;
use super::coll::coll;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::domain::sync_v2::ChangeAck;
use mongodb::bson;
use mongodb::bson::{DateTime as BsonDateTime, doc};

pub(crate) async fn store_batch_acks(
    db: &Db,
    device_id: &str,
    batch_id: &str,
    acks: &[ChangeAck],
) -> AppResult<()> {
    let acks = bson::serialize_to_bson(acks)
        .map_err(|e| AppError::internal(format!("cannot store batch acks: {e}")))?;
    // Upsert: a retry that raced the first attempt overwrites with the same acks.
    coll(db, BATCHES)?
        .update_one(
            doc! { "device_id": device_id, "batch_id": batch_id },
            doc! { "$set": { "acks": acks, "at": BsonDateTime::now() } },
        )
        .upsert(true)
        .await?;
    Ok(())
}
