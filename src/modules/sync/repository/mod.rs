//! Reads of the change log for the legacy changes endpoint.

mod sync_repository;

pub(crate) use sync_repository::fetch_collection_changes;
pub(crate) use sync_repository::fetch_latest_change_marker;
