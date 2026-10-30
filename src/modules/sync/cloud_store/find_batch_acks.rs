use super::batches::BATCHES;
use super::coll::coll;
use crate::clients::db::Db;

use crate::core::error::{AppError, AppResult};

use crate::domain::sync_v2::ChangeAck;
use mongodb::bson;
use mongodb::bson::doc;

/// Acks recorded for an already-processed batch, if any (push retry).
pub(crate) async fn find_batch_acks(
    db: &Db,
    device_id: &str,
    batch_id: &str,
) -> AppResult<Option<Vec<ChangeAck>>> {
    let Some(stored) = coll(db, BATCHES)?
        .find_one(doc! { "device_id": device_id, "batch_id": batch_id })
        .await?
    else {
        return Ok(None);
    };
    let acks = stored
        .get("acks")
        .cloned()
        .map(bson::deserialize_from_bson::<Vec<ChangeAck>>)
        .transpose()
        .map_err(|e| AppError::internal(format!("stored batch acks are unreadable: {e}")))?;
    Ok(acks)
}
