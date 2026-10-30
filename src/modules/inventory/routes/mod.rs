//! HTTP layer only: extractors, path/query parsing, and OpenAPI docs
//! (`#[utoipa::path]`). Every handler follows the same shape — extract
//! params, make exactly one `service::*` call, wrap the result in
//! `ApiResponse`/`StatusCode` — so individual handlers aren't commented
//! beyond that pattern; the business logic they call into lives in
//! `service/` and is commented there.

mod inventory_routes;

pub use inventory_routes::router;
