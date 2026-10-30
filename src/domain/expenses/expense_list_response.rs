use super::expense_row::ExpenseRow;

use serde::Serialize;
use utoipa::ToSchema;

/// A page of the combined list.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpenseListResponse {
    pub expenses: Vec<ExpenseRow>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}
