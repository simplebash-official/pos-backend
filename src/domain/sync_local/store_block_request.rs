use chrono::{DateTime, Utc};
use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StoreBlockRequest {
    /// Sequence name, e.g. `invoice`.
    pub name: String,
    pub prefix: String,
    pub padding: i64,
    pub start: i64,
    pub end: i64,
    pub expires_at: DateTime<Utc>,
}
