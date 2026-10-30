use super::payment_method_breakdown::PaymentMethodBreakdown;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Daily sales summary for a single calendar day.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailySalesReportSummary {
    /// Date in YYYY-MM-DD format.
    pub date: String,
    /// Total gross sales before discounts in cents.
    pub gross_sales_cents: i64,
    /// Total discounts given in cents.
    pub discount_cents: i64,
    /// Total net sales in cents (gross - discount).
    pub total_sales_cents: i64,
    /// Total number of invoices issued on this date.
    pub total_invoices: u64,
    /// Revenue from repair service jobs in cents.
    pub repair_revenue_cents: i64,
    /// Revenue from print jobs in cents.
    pub print_revenue_cents: i64,
    /// Revenue from retail merchandise in cents.
    pub retail_revenue_cents: i64,
    /// Payment method distributions on this date.
    pub payment_methods: PaymentMethodBreakdown,
}
