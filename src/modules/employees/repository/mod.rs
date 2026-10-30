//! Dual-engine (Mongo / SQLite) access for the `employees` collection.
//! Functions here return `Option`/`Vec` straight from the driver.

mod employees_repository;

pub(crate) use employees_repository::delete_employee;
pub(crate) use employees_repository::find_employee_by_id;
pub(crate) use employees_repository::find_employee_by_key;
pub(crate) use employees_repository::find_employees_by_keys;
pub(crate) use employees_repository::insert_employee;
pub(crate) use employees_repository::list_employees;
pub(crate) use employees_repository::touch_employee_by_key;
pub(crate) use employees_repository::update_employee;
