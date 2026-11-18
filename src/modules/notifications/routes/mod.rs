//! HTTP layer of the notification engine: the caller's history, bell summary, read state, live
//! stream and raising from the app, plus the internal route trusted internal services use.

mod internal_notifications;
mod notifications_routes;

pub use internal_notifications::internal_router;
pub use notifications_routes::router;
