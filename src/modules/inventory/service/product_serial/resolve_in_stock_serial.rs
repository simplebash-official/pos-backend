use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::SerialStatus;
use crate::modules::inventory::model::ProductSerialDocument;
use crate::modules::inventory::repository;

/// Resolves one serial number to an `InStock` unit of the given product for
/// a sale line — 404 `SERIAL_NOT_FOUND` if unknown/wrong product, 409
/// `SERIAL_ALREADY_SOLD` if it exists but isn't `InStock`.
pub(crate) async fn resolve_in_stock_serial(
    db: &Db,
    product_key: &str,
    serial_number: &str,
) -> AppResult<ProductSerialDocument> {
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

    if serial.status != SerialStatus::InStock {
        return Err(AppError::custom(
            axum::http::StatusCode::CONFLICT,
            codes::SERIAL_ALREADY_SOLD,
            format!("Serial number '{serial_number}' is not available to sell"),
        ));
    }
    Ok(serial)
}
