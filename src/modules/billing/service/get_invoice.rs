use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::billing::Invoice;

use crate::modules::billing::repository;

pub async fn get_invoice(db: &Db, id_or_key: &str) -> AppResult<Invoice> {
    let document = repository::find_invoice_by_id_or_key(db, id_or_key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Invoice not found", codes::INVOICE_NOT_FOUND)
        })?;
    Ok(document.into_invoice())
}
