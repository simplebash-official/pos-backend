// Pure business types for employee HR/commission profiles — no I/O, no
// Mongo/Axum types beyond serde/utoipa derives. Mongo document shape lives
// in `modules::employees::model` (private) and converts into `Employee`
// before a handler wraps it in `core::response::ApiResponse<T>`.
//
// An Employee is a separate entity from a `domain::users::User` login
// account — every staff member gets an Employee profile (HR/commission
// tracking), but a login is an optional add-on an Admin/Manager may later
// create for them (see `modules::users::service`'s `employee_key` linkage).
// `EmployeeRole` (job function) is unrelated to `domain::users::Role`
// (system access level) — a technician's `EmployeeRole` says nothing about
// whether they can log in at all, let alone what they could do if they did.

mod create_employee_request;
mod delete_employees_request;
mod delete_employees_response;
mod employee;
mod employee_earning_record;
mod employee_earnings_response;
mod employee_list_query;
mod employee_login_summary;
mod employee_role;
mod employee_status;
mod employees_response;
mod split_type;
mod update_employee_request;

pub use create_employee_request::CreateEmployeeRequest;
pub use delete_employees_request::DeleteEmployeesRequest;
pub use delete_employees_response::DeleteEmployeesResponse;
pub use employee::Employee;
pub use employee_earning_record::EmployeeEarningRecord;
pub use employee_earnings_response::EmployeeEarningsResponse;
pub use employee_list_query::EmployeeListQuery;
pub use employee_login_summary::EmployeeLoginSummary;
pub use employee_role::EmployeeRole;
pub use employee_status::EmployeeStatus;
pub use employees_response::EmployeesResponse;
pub use split_type::SplitType;
pub use update_employee_request::UpdateEmployeeRequest;
