use super::monthly_profit_day_entry::MonthlyProfitDayEntry;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Monthly Profit and Financial Intelligence report response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyProfitReportResponse {
    /// Year and month in YYYY-MM format.
    pub year_month: String,
    pub total_revenue_cents: i64,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub total_discounts_cents: i64,
    pub cogs_cents: i64,
    pub gross_profit_cents: i64,
    pub commission_payouts_cents: i64,
    pub net_profit_cents: i64,
    pub total_refunds_cents: i64,
    pub total_invoices_count: u64,
    pub daily_breakdown: Vec<MonthlyProfitDayEntry>,
}
