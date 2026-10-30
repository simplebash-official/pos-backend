#[derive(Debug, Clone, Default)]
pub(crate) struct SessionBillingSummary {
    pub total_invoices_count: u64,
    pub discount_cents: i64,
    pub cash_payment_count: u64,
    pub card_payment_count: u64,
    pub online_payment_count: u64,
    pub credit_invoice_count: u64,
}
