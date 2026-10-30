use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::domain::inventory::SerialStatus;

use crate::modules::inventory::repository;

use mongodb::bson::doc;
use mongodb::bson::oid::ObjectId;

/// Transitions a returned serial's status on credit-note creation, per the
/// plan's condition/disposition -> `SerialStatus` mapping:
/// `Resalable`/`OpenBoxDiscount` -> `ReturnedResalable`;
/// `Damaged`+`ReturnToSupplier` -> `UnderWarrantyClaim`;
/// `Damaged`+`WriteOffScrap` -> `WrittenOff`;
/// `Damaged`+`RepairPending` or `PendingInspection` -> `ReturnedFaulty`.
pub(crate) async fn transition_serial_on_return(
    db: &Db,
    serial_id: ObjectId,
    new_status: SerialStatus,
    credit_note_key: &str,
) -> AppResult<()> {
    let status_str = serde_json::to_value(new_status)
        .expect("a serial status serializes to JSON")
        .as_str()
        .expect("a serial status serializes to a string")
        .to_string();
    let set_doc = doc! {
        "status": status_str,
        "credit_note_key": credit_note_key,
    };
    repository::product_serial::update_serial(db, serial_id, set_doc).await?;
    Ok(())
}
