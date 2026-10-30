//! Dual-engine (Mongo / SQLite) access for the `account_openings` collection.

mod accounts_repository;

pub(crate) use accounts_repository::find_by_key;
pub(crate) use accounts_repository::insert;
pub(crate) use accounts_repository::save;
