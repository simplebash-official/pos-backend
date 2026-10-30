use super::category_sales_row::CategorySalesRow;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesByCategoryResponse {
    pub rows: Vec<CategorySalesRow>,
    /// Retail lines whose product no longer resolves (deleted) land here.
    pub uncategorised: CategorySalesRow,
    pub total_revenue_cents: i64,
    pub total_gross_profit_cents: i64,
}
