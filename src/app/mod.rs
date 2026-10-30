//! Application assembly: the shared `AppState`, the router with every module mounted, and the health endpoints.

mod app_items;
mod build_router;

pub use app_items::AppState;
pub use app_items::server_uptime_seconds;
pub use build_router::build_router;
