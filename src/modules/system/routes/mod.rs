//! HTTP surface of the system module: installation info and first-run setup.

mod system_routes;

pub use system_routes::SetupStatusQuery;
pub use system_routes::router;
