use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /billing/sessions/{key}/convert-variance`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConvertVarianceRequest {
    pub reason: String,
    pub recipient: String,
    pub manager_pin: String,
}
