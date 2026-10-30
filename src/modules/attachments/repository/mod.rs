//! Dual-engine (Mongo / SQLite) access for the `attachments` collection. Append-only.

mod attachments_repository;

pub(crate) use attachments_repository::find_by_key;
pub(crate) use attachments_repository::insert;
pub(crate) use attachments_repository::list_for_owner;
