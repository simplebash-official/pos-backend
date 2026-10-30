//! Pure business types for employee HR/commission profiles — no I/O, no
//! Mongo/Axum types beyond serde/utoipa derives. Mongo document shape lives
//! in `modules::employees::model` (private) and converts into `Employee`
//! before a handler wraps it in `core::response::ApiResponse<T>`.
//!
//! An Employee is a separate entity from a `domain::users::User` login
//! account — every staff member gets an Employee profile (HR/commission
//! tracking), but a login is an optional add-on an Admin/Manager may later
//! create for them (see `modules::users::service`'s `employee_key` linkage).
//! `EmployeeRole` (job function) is unrelated to `domain::users::Role`
//! (system access level) — a technician's `EmployeeRole` says nothing about
//! whether they can log in at all, let alone what they could do if they did.

mod domain_employees;

pub use domain_employees::CreateEmployeeRequest;
pub use domain_employees::DeleteEmployeesRequest;
pub use domain_employees::DeleteEmployeesResponse;
pub use domain_employees::Employee;
pub use domain_employees::EmployeeEarningRecord;
pub use domain_employees::EmployeeEarningsResponse;
pub use domain_employees::EmployeeListQuery;
pub use domain_employees::EmployeeLoginSummary;
pub use domain_employees::EmployeeRole;
pub use domain_employees::EmployeeStatus;
pub use domain_employees::EmployeesResponse;
pub use domain_employees::SplitType;
pub use domain_employees::UpdateEmployeeRequest;
