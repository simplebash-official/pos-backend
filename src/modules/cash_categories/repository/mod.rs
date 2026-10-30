//! Dual-engine (Mongo / SQLite) access for the `cash_categories` collection.
//! Returns documents straight from the driver; business rules live in `service`.

mod cash_categories_repository;

pub(crate) use cash_categories_repository::find_by_key;
pub(crate) use cash_categories_repository::insert;
pub(crate) use cash_categories_repository::list_all;
pub(crate) use cash_categories_repository::save;
