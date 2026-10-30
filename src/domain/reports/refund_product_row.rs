use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundProductRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    pub name: String,
    pub quantity: i64,
    pub amount_cents: i64,
}
