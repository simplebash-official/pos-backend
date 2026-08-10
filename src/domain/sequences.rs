use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReserveSequenceRequest {
    pub block_size: Option<u64>,
    pub device_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SequenceReservationResponse {
    pub name: String,
    pub block_id: String,
    pub prefix: String,
    pub padding: usize,
    pub start: u64,
    pub end: u64,
    pub expires_at: DateTime<Utc>,
}
