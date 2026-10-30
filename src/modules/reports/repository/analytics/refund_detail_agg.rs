// ---------------------------------------------------------------------------
// refunds (detailed)
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct RefundDetailAgg {
    pub credit_note_count: u64,
    pub net_refund_cents: i64,
    pub refund_cash_cents: i64,
    pub balance_reduction_cents: i64,
    pub exchange_count: u64,
    pub no_receipt_count: u64,
    pub manager_override_count: u64,
    pub by_reason: Vec<(String, u64, i64)>,
    pub by_method: Vec<(String, i64)>,
    pub top_products: Vec<(Option<String>, String, i64, i64)>,
}
