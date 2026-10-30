use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSyncStateRequest {
    pub clock_offset_ms: Option<i64>,
    pub linked: Option<bool>,
    pub tenant_id: Option<String>,
    pub cloud_cursor: Option<i64>,
    /// Clears an interrupted bootstrap so the next bootstrap page wipes again.
    pub bootstrap_reset: Option<bool>,
}
