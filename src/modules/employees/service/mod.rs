//! Business rules for employee HR/commission profile CRUD/listing: required-
//! field validation, the login-summary enrichment every read performs (a
//! cross-module call into `modules::users`), and the "still has a login"
//! guard that blocks deleting an employee while a login account still
//! references it. Delegates all Mongo access to `super::repository`.

mod employees_service;

pub use employees_service::create_employee;
pub(crate) use employees_service::delete_employee;
pub(crate) use employees_service::delete_employees;
pub(crate) use employees_service::get_employee;
pub(crate) use employees_service::get_employee_by_key;
pub(crate) use employees_service::get_employees_by_keys;
pub(crate) use employees_service::hydrate_sync_documents;
pub use employees_service::list_employees;
pub(crate) use employees_service::touch_by_key;
pub(crate) use employees_service::update_employee;
