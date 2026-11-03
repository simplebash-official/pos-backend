//! HTTP routes of the warranties module: `/api/warranties` and `/api/warranty-claims`.

mod claims_routes;
mod warranties_routes;

pub use claims_routes::router as claims_router;
pub use warranties_routes::router;
