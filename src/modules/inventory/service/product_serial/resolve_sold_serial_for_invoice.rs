use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::inventory::SerialStatus;
use crate::modules::inventory::model::ProductSerialDocument;
use crate::modules::inventory::repository;

/// Resolves a serial number to the specific `Sold` unit that was sold on
/// `invoice_key` — used when a credit-note line references a serialized
/// invoice line, to confirm the returned unit really was part of that sale.
pub(crate) async fn resolve_sold_serial_for_invoice(
    db: &Db,
    product_key: &str,
    serial_number: &str,
    invoice_key: &str,
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

    if serial.status != SerialStatus::Sold || serial.invoice_key.as_deref() != Some(invoice_key) {
        return Err(AppError::not_found_with_code(
            format!("Serial number '{serial_number}' was not sold on this invoice"),
            codes::SERIAL_NOT_FOUND,
        ));
    }
    Ok(serial)
}
