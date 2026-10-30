//! Dual-engine (Mongo / SQLite) access for the `expenses` and `expense_payments` collections.
//! Both are append-only: rows are inserted and read, never edited or deleted.

mod expenses_repository;

pub(crate) use expenses_repository::find_expense_by_key;
pub(crate) use expenses_repository::insert_expense;
pub(crate) use expenses_repository::insert_payment;
pub(crate) use expenses_repository::list_all_payments;
pub(crate) use expenses_repository::list_expenses;
pub(crate) use expenses_repository::list_payments_for;
