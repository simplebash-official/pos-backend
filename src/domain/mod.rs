// Pure business types shared across modules — no I/O, no framework types.
// Populated as domain modules (billing, repairs, inventory, ...) grow.

pub mod auth;
pub mod billing;
pub mod customers;
pub mod inventory;
pub mod print_jobs;
pub mod purchases;
pub mod repairs;
pub mod sequences;
pub mod supplier_products;
pub mod suppliers;
pub mod sync;
pub mod users;

use serde::Serialize;
use utoipa::ToSchema;

/// Payload for the top-level `/health` liveness check (see `app.rs`).
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
}

/// Payload every feature module's placeholder `GET /` status route returns
/// (built via `core::utils::module_status_response`) — a trivial "is this
/// module wired up" check, not a health/readiness probe.
#[derive(Debug, Serialize, ToSchema)]
pub struct ModuleStatusResponse {
    pub module: String,
    pub status: String,
}
