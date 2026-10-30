use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Valuation and stock breakdown for one category.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryValuationEntry {
    pub category_key: String,
    pub category_name: String,
    pub product_count: u64,
    pub total_units: i64,
    pub cost_valuation_cents: i64,
    pub retail_valuation_cents: i64,
    pub potential_profit_cents: i64,
}
