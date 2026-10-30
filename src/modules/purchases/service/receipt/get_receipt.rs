use super::assemble::assemble;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::purchase_receipts::PurchaseReceipt;

use crate::modules::purchases::repository;

/// One receipt with its lines.
pub async fn get_receipt(db: &Db, key: &str) -> AppResult<PurchaseReceipt> {
    let document = repository::receipt::find_receipt_by_key(db, key)
        .await?
        .ok_or_else(|| {
            AppError::not_found_with_code("Receipt not found", codes::RECEIPT_NOT_FOUND)
        })?;
    let lines =
        repository::list_purchases_by_receipts(db, std::slice::from_ref(&document.key)).await?;
    Ok(assemble(db, vec![(document, lines)]).await?.remove(0))
}
