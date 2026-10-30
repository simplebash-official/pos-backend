use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::{SerialLookup, SerialStatus};

use crate::modules::inventory::repository;

/// Resolves a scanned serial number (across all products) to its `InStock`
/// unit, its product and the price of its own batch — what a till needs to add
/// that exact unit to a sale. 404 `SERIAL_NOT_FOUND` for an unknown number, 409
/// `SERIAL_ALREADY_SOLD` when the unit is not in stock.
pub(crate) async fn lookup_in_stock_serial(
    db: &Db,
    serial_number: &str,
) -> AppResult<SerialLookup> {
    let serial = repository::product_serial::find_serial_by_number(db, serial_number)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code(
                format!("Serial number '{serial_number}' not found"),
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
    let product = super::super::product::get_product_by_key(db, &serial.product_key).await?;
    let batch = super::super::stock_batch::get_batch_for_product(
        db,
        &serial.batch_key,
        &serial.product_key,
    )
    .await?;
    Ok(SerialLookup {
        serial: serial.into_product_serial(),
        product,
        selling_price_cents: batch.selling_price_cents,
    })
}
