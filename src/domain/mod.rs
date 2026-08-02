// Pure business types shared across modules — no I/O, no framework types.
// Populated as domain modules (billing, repairs, inventory, ...) grow.

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ModuleStatusResponse {
    pub module: String,
    pub status: String,
}
