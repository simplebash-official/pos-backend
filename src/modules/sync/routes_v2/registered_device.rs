use serde::Serialize;
use utoipa::ToSchema;

/// Answer of `POST /sync/devices/register`.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredDevice {
    pub device_id: String,
    /// Highest change `seq` the tenant has; a fresh device bootstraps to it.
    pub server_seq: i64,
}
