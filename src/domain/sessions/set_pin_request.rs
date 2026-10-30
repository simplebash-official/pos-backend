use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /auth/set-pin`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetPinRequest {
    pub pin: String,
    #[serde(default)]
    pub target_user_id: Option<String>,
}
