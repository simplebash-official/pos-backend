// Cross-cutting infrastructure shared by every feature module: config
// loading, auth middleware, the `AppError`/`ApiResponse` envelopes, shared
// constants, and small request-handling utilities. Nothing in here is
// domain/business logic — that belongs in `modules::<name>::service`.

pub mod calculations;
pub mod config;
pub mod constants;
pub mod error;
pub mod id;
pub mod logging;
pub mod middleware;
pub mod openapi;
pub mod rate_limit;
pub mod response;
pub mod sync_merge;
pub mod sync_origin;
#[cfg(any(test, feature = "sync-sim"))]
pub mod sync_sim;
pub mod tenancy;
pub mod utils;
