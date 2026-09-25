// Pure business types shared across modules — no I/O, no framework types.
// Populated as domain modules (billing, repairs, inventory, ...) grow.

pub mod auth;
pub mod backup;
pub mod billing;
pub mod customers;
pub mod employees;
pub mod imports;
pub mod inventory;
pub mod print_jobs;
pub mod purchases;
pub mod repairs;
pub mod reports;
pub mod sequences;
pub mod supplier_products;
pub mod suppliers;
pub mod sync;
pub mod sync_local;
pub mod sync_v2;
pub mod system;
pub mod users;

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
