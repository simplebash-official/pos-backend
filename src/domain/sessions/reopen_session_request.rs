use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /billing/sessions/{key}/reopen`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReopenSessionRequest {
    pub reason: String,
}
