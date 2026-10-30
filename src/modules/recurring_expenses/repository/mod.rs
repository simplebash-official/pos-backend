//! Dual-engine (Mongo / SQLite) access for the `recurring_expenses` collection.

mod recurring_expenses_repository;

pub(crate) use recurring_expenses_repository::find_by_key;
pub(crate) use recurring_expenses_repository::insert;
pub(crate) use recurring_expenses_repository::list_all;
pub(crate) use recurring_expenses_repository::save;
