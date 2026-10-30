//! Business rules for supplier CRUD/listing: required-field validation,
//! lightweight phone/email format checks, and the "still referenced by
//! purchase history" guard that blocks deleting a supplier while purchases
//! still point at it by key. Delegates all Mongo access to `super::repository`.

mod suppliers_service;

pub use suppliers_service::create_supplier;
pub(crate) use suppliers_service::delete_supplier;
pub(crate) use suppliers_service::delete_suppliers;
pub(crate) use suppliers_service::get_supplier;
pub(crate) use suppliers_service::get_supplier_by_key;
pub(crate) use suppliers_service::get_supplier_categories;
pub use suppliers_service::get_supplier_stats;
pub(crate) use suppliers_service::get_suppliers_by_keys;
pub(crate) use suppliers_service::hydrate_sync_documents;
pub use suppliers_service::list_suppliers;
pub(crate) use suppliers_service::replace_supplier;
pub(crate) use suppliers_service::update_supplier;
