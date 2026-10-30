use super::month_total::MonthTotal;

use crate::domain::cash_categories::ReportGroup;
use serde::Serialize;
use utoipa::ToSchema;

/// One category's cost over the period, with its months.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryBreakdown {
    pub category_key: String,
    pub category_name: String,
    pub report_group: ReportGroup,
    pub total_cents: i64,
    /// Share of the period's total cost, whole percent rounded half up.
    pub share_percent: i64,
    pub months: Vec<MonthTotal>,
}
