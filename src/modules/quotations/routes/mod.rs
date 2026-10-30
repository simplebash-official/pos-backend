//! HTTP layer for quotations. Every route requires `quotations:manage`
//! (Admin and Manager only), checked as the first line of each handler — a
//! Staff token gets 403 here no matter what the UI shows.

mod quotations_routes;

pub use quotations_routes::router;
