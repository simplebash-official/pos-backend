use super::top_product_entry::TopProductEntry;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Top Products report response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopProductsResponse {
    pub products: Vec<TopProductEntry>,
    pub total_units_sold: i64,
    pub total_revenue_cents: i64,
}
