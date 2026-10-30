// ---------------------------------------------------------------------------
// discounts
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct DiscountAgg {
    pub gross_before_discount_cents: i64,
    pub order_level_discount_cents: i64,
    pub line_level_discount_cents: i64,
    pub invoice_count: u64,
    pub invoices_with_discount: u64,
    pub by_type: Vec<(String, u64, i64)>,
    pub by_cashier: Vec<(String, String, i64, i64)>,
    pub top_products: Vec<(Option<String>, String, i64, i64)>,
}
