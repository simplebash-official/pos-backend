use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::billing::PaymentRecord;

use crate::modules::billing::repository;

pub async fn list_payments(db: &Db, invoice_key: &str) -> AppResult<Vec<PaymentRecord>> {
    // 404s if the invoice itself doesn't exist, so a typo'd key is
    // distinguishable from a real invoice with no payments yet — same
    // convention as `inventory::service::stock::product_movements`.
    repository::find_invoice_by_key(db, invoice_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
        })?;

    let documents = repository::list_payments_for_invoice(db, invoice_key).await?;
    Ok(documents
        .into_iter()
        .map(|d| d.into_payment_record())
        .collect())
}
