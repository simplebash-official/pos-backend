//! HTTP layer only — mirrors `modules::repairs::routes` exactly.

mod job_receipt_routes;
mod print_jobs_routes;

pub use print_jobs_routes::router;
