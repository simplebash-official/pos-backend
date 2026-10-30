// Dual-engine (Mongo / SQLite) access for the `employees` collection.
// Functions here return `Option`/`Vec` straight from the driver.

mod delete_employee;
mod employee_sqlite_row;
mod employees;
mod find_employee_by_id;
mod find_employee_by_key;
mod find_employees_by_keys;
mod insert_employee;
mod list_employees;
mod touch_employee_by_key;
mod update_employee;

pub(crate) use delete_employee::delete_employee;
pub(crate) use find_employee_by_id::find_employee_by_id;
pub(crate) use find_employee_by_key::find_employee_by_key;
pub(crate) use find_employees_by_keys::find_employees_by_keys;
pub(crate) use insert_employee::insert_employee;
pub(crate) use list_employees::list_employees;
pub(crate) use touch_employee_by_key::touch_employee_by_key;
pub(crate) use update_employee::update_employee;
