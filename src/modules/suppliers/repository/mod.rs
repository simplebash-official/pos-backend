//! Dual-engine (Mongo / SQLite) access for the `suppliers` collection.
//! Functions here return `Option`/`Vec`/counts straight from the driver.

mod suppliers_repository;

pub(crate) use suppliers_repository::count_stats;
pub(crate) use suppliers_repository::delete_supplier;
pub(crate) use suppliers_repository::distinct_supplied_categories;
pub(crate) use suppliers_repository::find_supplier_by_id;
pub(crate) use suppliers_repository::find_supplier_by_key;
pub(crate) use suppliers_repository::find_suppliers_by_keys;
pub(crate) use suppliers_repository::insert_supplier;
pub(crate) use suppliers_repository::list_suppliers;
pub(crate) use suppliers_repository::update_supplier;
