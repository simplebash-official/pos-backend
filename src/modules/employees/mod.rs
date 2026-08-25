// Employee HR/commission profiles — every staff member gets one, login or
// not. `repository` is private so nothing outside this module tree reaches
// past `service` into persistence (see `modules::suppliers` for the pattern
// this mirrors); `model` stays `pub` like suppliers', for integration tests
// to seed fixture data directly. `service` is `pub` so sibling modules can
// reach in: `repairs`/`print_jobs` resolve an `assignedEmployeeId` via
// `service::get_employee_by_key`, `reports` batch-resolves commission
// entries via `service::get_employees_by_keys`, and `modules::users` calls
// in for `employeeKey` validation on login creation (with `employees`
// calling back into `users` for login-summary enrichment — see
// `users::service::find_user_summary_by_employee_key`).
pub mod model;
mod repository;
pub mod routes;
pub mod service;
