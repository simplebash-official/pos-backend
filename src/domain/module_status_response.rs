use serde::Serialize;
use utoipa::ToSchema;

/// Payload every feature module's placeholder `GET /` status route returns
/// (built via `core::utils::module_status_response`) — a trivial "is this
/// module wired up" check, not a health/readiness probe.
#[derive(Debug, Serialize, ToSchema)]
pub struct ModuleStatusResponse {
    /// Name of the module.
    pub module: String,
    /// Module status string (e.g. "active").
    pub status: String,
}
