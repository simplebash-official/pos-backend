use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PatternBucket {
    pub bucket: u8,
    pub label: String,
    pub invoice_count: u64,
    pub revenue_cents: i64,
}
