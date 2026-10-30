// HTTP layer only: extractors, path/query parsing, and OpenAPI docs
// (`#[utoipa::path]`). Every handler follows the same shape — extract
// params, make exactly one `service::*` call, wrap the result in
// `ApiResponse`/`StatusCode` — the business logic lives in `service/`.

mod create_customer;
mod delete_customer;
mod delete_customers_batch;
mod get_customer;
mod get_customer_stats;
mod get_customer_tags;
mod list_customers;
mod replace_customer;
mod router;
mod update_customer;

pub use router::router;
