use super::category_breakdown::CategoryBreakdown;
use super::chart_series::ChartSeries;
use super::month_total::MonthTotal;

use serde::Serialize;
use utoipa::ToSchema;

/// Response of `GET /finance-reports/expenses`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseBreakdownResponse {
    pub from: String,
    pub to: String,
    pub total_cents: i64,
    /// Biggest first.
    pub categories: Vec<CategoryBreakdown>,
    /// The biggest categories, then one "other" band for the rest: what the monthly chart stacks.
    pub chart: Vec<ChartSeries>,
    /// Every month in the period, including months with no cost, oldest first.
    pub months: Vec<MonthTotal>,
}
