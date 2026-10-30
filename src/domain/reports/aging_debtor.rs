use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgingDebtor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    pub outstanding_cents: i64,
    pub oldest_days: i32,
    pub invoice_count: u64,
}
