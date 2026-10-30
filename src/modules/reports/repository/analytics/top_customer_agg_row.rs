use chrono::{DateTime, Utc};

// ---------------------------------------------------------------------------
// top customers
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Default)]
pub(crate) struct TopCustomerAggRow {
    pub customer_key: Option<String>,
    pub customer_name: String,
    pub customer_phone: Option<String>,
    pub invoice_count: u64,
    pub revenue_cents: i64,
    pub discount_cents: i64,
    /// retail line revenue − retail COGS for this customer (product margin only).
    pub gross_profit_cents: i64,
    pub last_purchase_at: DateTime<Utc>,
}
