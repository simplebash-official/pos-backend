//! HTTP layer only: extractors, path/query parsing, and OpenAPI docs
//! (`#[utoipa::path]`). Every handler follows the same shape — extract
//! params, make exactly one `service::*` call, wrap the result in
//! `ApiResponse`/`StatusCode` — the business logic lives in `service/`.

mod customers_routes;

pub use customers_routes::router;
