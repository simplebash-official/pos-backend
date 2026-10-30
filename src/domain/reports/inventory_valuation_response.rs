use super::category_valuation_entry::CategoryValuationEntry;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Full inventory valuation report payload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryValuationResponse {
    pub total_products_count: u64,
    pub total_stock_units: i64,
    pub total_cost_valuation_cents: i64,
    pub total_retail_valuation_cents: i64,
    pub potential_gross_profit_cents: i64,
    pub potential_margin_percentage: f64,
    pub low_stock_products_count: u64,
    pub out_of_stock_products_count: u64,
    pub categories: Vec<CategoryValuationEntry>,
}
