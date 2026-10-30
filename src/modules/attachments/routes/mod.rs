//! HTTP layer for receipt photos. Who may see or add a photo follows the record it belongs to.

mod attachments_routes;

pub use attachments_routes::router;
