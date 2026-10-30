use crate::clients::uow::Unit;
use crate::core::constants::prefixes;
use crate::core::error::AppResult;
use crate::core::id::generate_id;
use crate::domain::inventory::SerialStatus;
use crate::modules::inventory::model::ProductSerialDocument;
use crate::modules::inventory::repository;

use mongodb::bson::DateTime as BsonDateTime;

/// Mints `ProductSerialDocument`s for units received on a purchase receipt,
/// each tied to the stock batch the purchase created (a unit sells at its
/// batch's price). Callers check uniqueness first (`ensure_serials_untracked`);
/// inside a transaction the unique index on the serial number is the backstop
/// and a duplicate rolls the whole receipt back.
pub(crate) async fn create_serials_for_purchase(
    unit: impl Into<Unit<'_>>,
    product_key: &str,
    batch_key: &str,
    serial_numbers: &[String],
) -> AppResult<()> {
    let mut unit = unit.into();

    let now = BsonDateTime::now();
    for serial_number in serial_numbers {
        let document = ProductSerialDocument {
            id: None,
            key: generate_id(prefixes::PRODUCT_SERIAL),
            product_key: product_key.to_string(),
            serial_number: serial_number.clone(),
            batch_key: batch_key.to_string(),
            status: SerialStatus::InStock,
            invoice_key: None,
            sold_at: None,
            warranty_months: None,
            warranty_expires_at: None,
            credit_note_key: None,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        repository::product_serial::insert_product_serial(&mut unit, document).await?;
    }
    Ok(())
}
