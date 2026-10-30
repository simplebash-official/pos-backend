use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Engine cache invalidation response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EngineInvalidateResponse {
    pub invalidated_keys_count: usize,
    pub message: String,
}
