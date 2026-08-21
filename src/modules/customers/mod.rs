pub mod model;
mod repository;
pub mod routes;
// `pub` rather than `pub(crate)`: `modules::sync` calls
// `service::hydrate_sync_documents` to convert raw customer documents into
// the same DTO the REST reads return, and the integration test suite calls
// `service::get_customer_stats` directly against an isolated throwaway
// database (see `tests/customers_test.rs` — the same pattern
// `billing`/`repairs`/`print_jobs`/`suppliers` already use for their own
// stats tests, which is why those modules' `service` is `pub` too).
pub mod service;
