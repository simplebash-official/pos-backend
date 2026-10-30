use super::category_total::CategoryTotal;

use crate::domain::cash_categories::ReportGroup;
use serde::Serialize;
use utoipa::ToSchema;

/// What the shop spent in one group of costs (running costs, staff pay), with its categories.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseGroupTotal {
    pub group: ReportGroup,
    pub total_cents: i64,
    pub categories: Vec<CategoryTotal>,
}
