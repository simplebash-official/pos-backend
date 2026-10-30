use serde::Serialize;
use utoipa::ToSchema;

/// Payload for the top-level `/health` liveness check (see `app.rs`).
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// Overall application health status (e.g. "ok").
    pub status: String,
    /// Service uptime in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_seconds: Option<u64>,
    /// Environment (e.g. "development" or "production").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// Application package version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}
