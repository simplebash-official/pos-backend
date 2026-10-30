use serde::Deserialize;
use utoipa::ToSchema;

/// Body of `POST /api/sync/enable`: the cloud-assigned identity of this device.
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnableRequest {
    pub tenant_id: Option<String>,
    /// Replaces the locally generated device id with the one the cloud issued.
    pub device_id: Option<String>,
}
