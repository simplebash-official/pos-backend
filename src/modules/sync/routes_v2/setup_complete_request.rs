use serde::Deserialize;
use utoipa::ToSchema;

/// Body of `POST /sync/setup-complete`.
#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupCompleteRequest {
    /// The device's own setup loaded the demo data.
    #[serde(default)]
    pub sample_data_loaded: bool,
}
