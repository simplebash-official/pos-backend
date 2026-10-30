use serde::Deserialize;
use utoipa::ToSchema;

/// Request body for reserving a block of sequence numbers.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReserveSequenceRequest {
    /// Desired number of sequence numbers to reserve in this block.
    pub block_size: Option<u64>,
    /// Unique identifier of device requesting the sequence block.
    pub device_id: Option<String>,
}
