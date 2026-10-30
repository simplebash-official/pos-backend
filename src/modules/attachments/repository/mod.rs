// Dual-engine (Mongo / SQLite) access for the `attachments` collection. Append-only.

mod attachments;
mod find_by_key;
mod from_row;
mod insert;
mod list_for_owner;

pub(crate) use find_by_key::find_by_key;
pub(crate) use insert::insert;
pub(crate) use list_for_owner::list_for_owner;
