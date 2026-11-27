//! HTTP layer only — mirrors `modules::repairs::routes` exactly.

mod print_frame_sizes_routes;
mod print_jobs_routes;
mod print_services_routes;

pub use print_frame_sizes_routes::router as frame_sizes_router;
pub use print_jobs_routes::router;
pub use print_services_routes::router as services_router;
