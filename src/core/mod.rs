// Cross-cutting infrastructure shared by every feature module: config
// loading, auth middleware, the `AppError`/`ApiResponse` envelopes, shared
// constants, and small request-handling utilities. Nothing in here is
// domain/business logic — that belongs in `modules::<name>::service`.

pub mod config;
pub mod constants;
pub mod error;
pub mod id;
pub mod middleware;
pub mod openapi;
pub mod response;
pub mod utils;
