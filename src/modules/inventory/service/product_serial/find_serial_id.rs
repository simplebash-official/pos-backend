use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::modules::inventory::repository;

use mongodb::bson::oid::ObjectId;

/// Resolves a serial number + product to its Mongo `ObjectId`, regardless of
/// current lifecycle status — used by a credit-note return, which needs to
/// transition a unit that's currently `Sold` (not `InStock`).
pub(crate) async fn find_serial_id(
    db: &Db,
    product_key: &str,
    serial_number: &str,
) -> AppResult<ObjectId> {
    let serial = repository::product_serial::find_serial_by_number_and_product(
        db,
        serial_number,
        product_key,
    )
    .await?
    .ok_or_else(|| {
        AppError::not_found_with_code(
            format!("Serial number '{serial_number}' not found for this product"),
            codes::SERIAL_NOT_FOUND,
        )
    })?;
    serial
        .id
        .ok_or_else(|| AppError::internal("persisted product serial document must have an id"))
}
