//! Business rules of the notification engine: raising a notification (deduplicated, inside the
//! producer's transaction), who may see and address what, the per-user read state and retention.

mod audience;
mod notifications_service;

pub use notifications_service::NOTIFICATION_RETENTION_DAYS;
pub use notifications_service::create_from_app;
pub use notifications_service::create_from_service;
pub use notifications_service::list;
pub use notifications_service::mark_all_read;
pub use notifications_service::mark_read;
pub(crate) use notifications_service::publish;
pub use notifications_service::purge_expired;
pub use notifications_service::summary;
