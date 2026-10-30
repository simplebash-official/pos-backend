// Dual-engine (Mongo / SQLite) access for the `suppliers` collection.
// Functions here return `Option`/`Vec`/counts straight from the driver.

mod count_stats;
mod delete_supplier;
mod distinct_supplied_categories;
mod find_supplier_by_id;
mod find_supplier_by_key;
mod find_suppliers_by_keys;
mod insert_supplier;
mod list_suppliers;
mod supplier_sqlite_row;
mod suppliers;
mod update_supplier;

pub(crate) use count_stats::count_stats;
pub(crate) use delete_supplier::delete_supplier;
pub(crate) use distinct_supplied_categories::distinct_supplied_categories;
pub(crate) use find_supplier_by_id::find_supplier_by_id;
pub(crate) use find_supplier_by_key::find_supplier_by_key;
pub(crate) use find_suppliers_by_keys::find_suppliers_by_keys;
pub(crate) use insert_supplier::insert_supplier;
pub(crate) use list_suppliers::list_suppliers;
pub(crate) use update_supplier::update_supplier;
