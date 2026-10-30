use super::daily_sales_report_summary::DailySalesReportSummary;
use super::payment_method_breakdown::PaymentMethodBreakdown;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Daily Sales report payload with daily entries and aggregate totals.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailySalesReportResponse {
    pub summaries: Vec<DailySalesReportSummary>,
    pub total_sales_cents: i64,
    pub total_invoices: u64,
    pub total_repair_revenue_cents: i64,
    pub total_print_revenue_cents: i64,
    pub total_retail_revenue_cents: i64,
    pub total_discounts_cents: i64,
    pub payment_methods: PaymentMethodBreakdown,
}
