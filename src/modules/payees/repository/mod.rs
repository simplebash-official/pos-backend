//! Dual-engine (Mongo / SQLite) access for the `payees` collection.

mod payees_repository;

pub(crate) use payees_repository::find_by_key;
pub(crate) use payees_repository::insert;
pub(crate) use payees_repository::list_all;
pub(crate) use payees_repository::save;
