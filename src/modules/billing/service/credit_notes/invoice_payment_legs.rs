use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::billing::repository;

/// What an invoice has been paid so far, one entry per payment (method, cents), refunds being
/// negative. The refund preview caps the cash refund at their sum and splits it across them.
pub async fn invoice_payment_legs(db: &Db, invoice_key: &str) -> AppResult<Vec<(String, i64)>> {
    Ok(repository::list_payments_for_invoice(db, invoice_key)
        .await?
        .into_iter()
        .map(|p| (p.payment_method, p.amount_cents))
        .collect())
}
