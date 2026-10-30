use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgingBucket {
    pub label: String,
    pub min_days: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_days: Option<i32>,
    pub invoice_count: u64,
    pub amount_cents: i64,
}
