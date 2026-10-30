use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /auth/verify-pin`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyPinRequest {
    pub pin: String,
    #[serde(default)]
    pub required_role: Option<String>,
}
