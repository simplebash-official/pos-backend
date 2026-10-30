use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// --- cashier performance ------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashierPerformanceEntry {
    pub cashier_id: String,
    pub cashier_name: String,
    pub invoice_count: u64,
    pub revenue_cents: i64,
    pub retail_revenue_cents: i64,
    pub items_sold: i64,
    pub discount_given_cents: i64,
    pub discount_rate_bps: i64,
    pub avg_basket_cents: i64,
    pub credit_invoice_count: u64,
    pub refund_count: u64,
    pub refunded_cents: i64,
    pub gross_profit_cents: i64,
}
