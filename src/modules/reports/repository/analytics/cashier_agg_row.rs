// ---------------------------------------------------------------------------
// cashier performance
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct CashierAggRow {
    pub cashier_id: String,
    pub cashier_name: String,
    pub invoice_count: u64,
    pub revenue_cents: i64,
    pub retail_revenue_cents: i64,
    pub items_sold: i64,
    pub discount_given_cents: i64,
    pub credit_invoice_count: u64,
    pub gross_profit_cents: i64,
}
