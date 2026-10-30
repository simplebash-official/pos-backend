// Dual-engine (Mongo / SQLite) access for the `incomes` collection.
// Append-only: rows are inserted and read, never edited or deleted.

mod find_income_by_key;
mod income_from_row;
mod incomes;
mod insert_income;
mod list_incomes;

pub(crate) use find_income_by_key::find_income_by_key;
pub(crate) use insert_income::insert_income;
pub(crate) use list_incomes::list_incomes;
