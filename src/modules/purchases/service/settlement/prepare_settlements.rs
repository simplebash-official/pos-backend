use super::prepared_settlements::PreparedSettlements;
use crate::clients::db::Db;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::purchase_receipts::ReceiptSettlementRequest;

use crate::modules::purchases::service::receipt::get_receipt;

use std::collections::HashSet;

/// Checks that every named receipt belongs to this supplier and is owed at least what is put
/// against it, and that the parts add up to no more than the payment. Nothing is written.
pub(crate) async fn prepare_settlements(
    db: &Db,
    supplier_key: &str,
    payment_cents: i64,
    requests: &[ReceiptSettlementRequest],
) -> AppResult<PreparedSettlements> {
    let mut seen = HashSet::new();
    let mut total = 0;
    let mut parts = Vec::with_capacity(requests.len());
    for request in requests {
        if request.amount_cents <= 0 {
            return Err(AppError::validation(
                "The amount put against a receipt must be more than zero",
            ));
        }
        if !seen.insert(request.receipt_key.clone()) {
            return Err(AppError::validation("A receipt can be listed only once"));
        }
        let receipt = get_receipt(db, &request.receipt_key).await?;
        if receipt.supplier_key != supplier_key {
            return Err(AppError::validation(
                "That receipt belongs to another supplier",
            ));
        }
        if request.amount_cents > receipt.due_cents {
            return Err(AppError::conflict(
                codes::RECEIPT_OVERPAID,
                format!(
                    "{} is owed less than that.",
                    receipt
                        .reference_no
                        .as_deref()
                        .unwrap_or(&receipt.receipt_number)
                ),
            ));
        }
        total += request.amount_cents;
        parts.push((request.receipt_key.clone(), request.amount_cents));
    }
    if total > payment_cents {
        return Err(AppError::validation(
            "More is put against receipts than is being paid",
        ));
    }
    Ok(PreparedSettlements {
        supplier_key: supplier_key.to_string(),
        parts,
    })
}
