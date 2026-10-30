// Business rules for employee HR/commission profile CRUD/listing: required-
// field validation, the login-summary enrichment every read performs (a
// cross-module call into `modules::users`), and the "still has a login"
// guard that blocks deleting an employee while a login account still
// references it. Delegates all Mongo access to `super::repository`.

mod create_employee;
mod delete_employee;
mod delete_employees;
mod get_employee;
mod get_employee_by_key;
mod get_employees_by_keys;
mod hydrate_sync_documents;
mod list_employees;
mod touch_by_key;
mod update_employee;
mod validate_required_fields;

pub use create_employee::create_employee;
pub(crate) use delete_employee::delete_employee;
pub(crate) use delete_employees::delete_employees;
pub(crate) use get_employee::get_employee;
pub(crate) use get_employee_by_key::get_employee_by_key;
pub(crate) use get_employees_by_keys::get_employees_by_keys;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_employees::list_employees;
pub(crate) use touch_by_key::touch_by_key;
pub(crate) use update_employee::update_employee;
