use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Line item for a ranked top-selling product.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopProductEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    pub source_type: String,
    pub units_sold: i64,
    pub total_revenue_cents: i64,
    pub total_discount_cents: i64,
}
