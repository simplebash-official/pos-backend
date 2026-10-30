// Business rules for supplier CRUD/listing: required-field validation,
// lightweight phone/email format checks, and the "still referenced by
// purchase history" guard that blocks deleting a supplier while purchases
// still point at it by key. Delegates all Mongo access to `super::repository`.

mod create_supplier;
mod delete_supplier;
mod delete_suppliers;
mod get_supplier;
mod get_supplier_by_key;
mod get_supplier_categories;
mod get_supplier_stats;
mod get_suppliers_by_keys;
mod hydrate_sync_documents;
mod list_suppliers;
mod replace_supplier;
mod resolve_and_validate_contacts;
mod sanitize_string;
mod update_supplier;
mod validate_contact_persons;
mod validate_email;
mod validate_phone;
mod validate_required_fields;

pub use create_supplier::create_supplier;
pub(crate) use delete_supplier::delete_supplier;
pub(crate) use delete_suppliers::delete_suppliers;
pub(crate) use get_supplier::get_supplier;
pub(crate) use get_supplier_by_key::get_supplier_by_key;
pub(crate) use get_supplier_categories::get_supplier_categories;
pub use get_supplier_stats::get_supplier_stats;
pub(crate) use get_suppliers_by_keys::get_suppliers_by_keys;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_suppliers::list_suppliers;
pub(crate) use replace_supplier::replace_supplier;
pub(crate) use update_supplier::update_supplier;
