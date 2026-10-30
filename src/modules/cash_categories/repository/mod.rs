// Dual-engine (Mongo / SQLite) access for the `cash_categories` collection.
// Returns documents straight from the driver; business rules live in `service`.

mod categories;
mod find_by_key;
mod from_row;
mod insert;
mod list_all;
mod save;

pub(crate) use find_by_key::find_by_key;
pub(crate) use insert::insert;
pub(crate) use list_all::list_all;
pub(crate) use save::save;
