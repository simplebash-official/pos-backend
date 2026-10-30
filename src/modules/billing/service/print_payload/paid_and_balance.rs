use crate::domain::billing::Invoice;

/// How much has been paid against this invoice, and how much is still owed,
/// for the printed "Amount Paid" / "Balance Due" lines. `paid_so_far_cents`
/// is the sum of every payment row (checkout deposit + later installments);
/// callers that don't have it can pass `invoice.amount_received_cents`.
pub(super) fn paid_and_balance(invoice: &Invoice, paid_so_far_cents: i64) -> (i64, i64) {
    crate::core::calculations::invoice_paid_and_balance(
        invoice.is_credit,
        invoice.total_cents,
        paid_so_far_cents,
        invoice.refunded_cents,
    )
}
