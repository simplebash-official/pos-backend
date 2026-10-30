//! Dual-engine (Mongo / SQLite) access for the `incomes` collection.
//! Append-only: rows are inserted and read, never edited or deleted.

mod incomes_repository;

pub(crate) use incomes_repository::find_income_by_key;
pub(crate) use incomes_repository::insert_income;
pub(crate) use incomes_repository::list_incomes;
