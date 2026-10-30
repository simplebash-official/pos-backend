use super::coll::coll;
use super::devices::DEVICES;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use mongodb::bson::{DateTime as BsonDateTime, doc};
use mongodb::options::ReturnDocument;

/// Whether the device is registered for the tenant and not revoked. A device
/// seen for the first time is registered on the spot (token issuance already
/// authenticated it), so `Ok(true)` also covers "new".
pub(crate) async fn ensure_device_active(db: &Db, device_id: &str) -> AppResult<bool> {
    // A plain read on the hot path (every pull); the upsert below only runs
    // the first time a device shows up.
    if let Some(known) = coll(db, DEVICES)?
        .find_one(doc! { "device_id": device_id })
        .await?
    {
        return Ok(!known.get_bool("revoked").unwrap_or(false));
    }
    let state = coll(db, DEVICES)?
        .find_one_and_update(
            doc! { "device_id": device_id },
            doc! { "$setOnInsert": {
                "device_id": device_id,
                "revoked": false,
                "last_outbox_seq": 0_i64,
                "cloud_cursor": 0_i64,
                "registered_at": BsonDateTime::now(),
            }},
        )
        .upsert(true)
        .return_document(ReturnDocument::After)
        .await?;
    Ok(!state
        .and_then(|d| d.get_bool("revoked").ok())
        .unwrap_or(false))
}
