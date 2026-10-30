use serde::Serialize;
use utoipa::ToSchema;

/// Answer of `POST /sync/setup-complete`.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupCompleteResponse {
    /// Always true once the call succeeds.
    pub setup_completed: bool,
    /// False when the shop was already set up (nothing was changed).
    pub changed: bool,
}
