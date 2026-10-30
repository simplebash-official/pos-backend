use serde::Deserialize;
use utoipa::ToSchema;

/// Body of `POST /sync/devices/register`, as every desktop app sends it.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegisterDeviceRequest {
    /// The desktop app's version (`"1.14.0"`); too old a version cannot sync.
    pub app_version: String,
}
