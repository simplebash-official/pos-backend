use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /sessions/movements/{key}/reverse`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReverseCashMovementRequest {
    /// Required once the undo window has passed; optional inside it.
    #[serde(default)]
    pub reason: Option<String>,
}
