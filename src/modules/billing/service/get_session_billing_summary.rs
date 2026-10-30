use super::session_billing_summary::SessionBillingSummary;
use crate::clients::db::Db;

use crate::core::error::AppResult;

use crate::modules::billing::repository;

/// Aggregates sales invoice counts, discounts, and tender transaction counts for a register session.
pub(crate) async fn get_session_billing_summary(
    db: &Db,
    session_key: &str,
) -> AppResult<SessionBillingSummary> {
    let (invoices, payments) =
        repository::find_session_invoices_and_payments(db, session_key).await?;

    let total_invoices_count = invoices.len() as u64;
    let discount_cents = invoices.iter().map(|inv| inv.discount_cents).sum();
    let credit_invoice_count = invoices
        .iter()
        .filter(|inv| inv.is_credit || inv.payment_method == "credit")
        .count() as u64;

    let mut cash_payment_count = 0u64;
    let mut card_payment_count = 0u64;
    let mut online_payment_count = 0u64;

    for payment in &payments {
        match payment.payment_method.as_str() {
            "cash" => cash_payment_count += 1,
            "card" => card_payment_count += 1,
            "online" => online_payment_count += 1,
            _ => {}
        }
    }

    Ok(SessionBillingSummary {
        total_invoices_count,
        discount_cents,
        cash_payment_count,
        card_payment_count,
        online_payment_count,
        credit_invoice_count,
    })
}
