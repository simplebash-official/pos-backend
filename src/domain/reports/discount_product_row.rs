use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscountProductRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    pub name: String,
    pub discount_cents: i64,
    pub units_sold: i64,
}
