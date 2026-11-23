//! Dual-engine (MongoDB / SQLite) access for `notifications` and `notification_reads`.

mod history_repository;
mod notifications_repository;

pub(crate) use history_repository::Selection;
pub(crate) use history_repository::Viewer;
pub(crate) use history_repository::list_visible;
pub(crate) use history_repository::unread_by_category;
pub(crate) use history_repository::visible_among;
pub(crate) use history_repository::visible_keys;
pub(crate) use notifications_repository::find_by_key;
pub(crate) use notifications_repository::insert;
pub(crate) use notifications_repository::insert_reads_if_absent;
pub(crate) use notifications_repository::purge_older_than;
