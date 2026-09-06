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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// An employee's job function — purely an HR/commission categorization,
/// carries no system-permission meaning (that's `domain::users::Role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmployeeRole {
    Technician,
    Printer,
    Sales,
    General,
}

impl EmployeeRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            EmployeeRole::Technician => "technician",
            EmployeeRole::Printer => "printer",
            EmployeeRole::Sales => "sales",
            EmployeeRole::General => "general",
        }
    }
}

impl std::str::FromStr for EmployeeRole {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "technician" => Ok(EmployeeRole::Technician),
            "printer" => Ok(EmployeeRole::Printer),
            "sales" => Ok(EmployeeRole::Sales),
            "general" => Ok(EmployeeRole::General),
            other => Err(format!("Unknown employee role: {other}")),
        }
    }
}

/// How an employee's commission on a job is calculated: a percentage of
/// profit, or a fixed amount per job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SplitType {
    Percentage,
    Fixed,
}

impl SplitType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SplitType::Percentage => "percentage",
            SplitType::Fixed => "fixed",
        }
    }
}

impl std::str::FromStr for SplitType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "percentage" => Ok(SplitType::Percentage),
            "fixed" => Ok(SplitType::Fixed),
            other => Err(format!("Unknown split type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum EmployeeStatus {
    Active,
    Inactive,
}

impl EmployeeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            EmployeeStatus::Active => "active",
            EmployeeStatus::Inactive => "inactive",
        }
    }
}

impl std::str::FromStr for EmployeeStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(EmployeeStatus::Active),
            "inactive" => Ok(EmployeeStatus::Inactive),
            other => Err(format!("Unknown employee status: {other}")),
        }
    }
}

/// Summary of an employee's linked login account, resolved live at read time
/// via `modules::users::service::find_user_summary_by_employee_key` — `None`
/// when the employee has no login (the common case for e.g. a repair
/// technician tracked for commission but never signing in). Never persisted
/// on the employee document itself; there is deliberately no reverse
/// `user_key` field — "does this employee have a login" is always resolved
/// live against the `users` collection, the same graceful-degradation
/// `Option` pattern `Purchase`'s live-resolved `supplier`/`product` uses.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeLoginSummary {
    pub user_id: String,
    pub email: String,
    pub role: crate::domain::users::Role,
    pub is_active: bool,
}

/// An employee HR/commission profile as returned to API clients. `id` (the
/// Mongo `ObjectId` as a hex string) is the route/lookup key; `key` is the
/// human-shareable prefixed id used by `repairs`/`print_jobs` to reference
/// an assigned employee, and by `reports::employee_earnings` to resolve
/// commission history.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Employee {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key identifying this employee (e.g. emp_...).
    pub key: String,
    /// Full display name.
    pub name: String,
    /// Contact phone number.
    pub phone: String,
    /// National ID / NIC number, if recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nic_or_id: Option<String>,
    /// Job function (technician, printer, sales, or general).
    pub role: EmployeeRole,
    /// Default commission split method applied when this employee is
    /// assigned to a repair/print job, unless overridden per-job.
    pub default_split_type: SplitType,
    /// Default commission split value (percentage or fixed cents,
    /// depending on `default_split_type`).
    pub default_split_value: f64,
    /// Whether this employee is currently active.
    pub status: EmployeeStatus,
    /// Internal notes or remarks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Linked login account summary, resolved live — see
    /// `EmployeeLoginSummary`. `None` when this employee has no login.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login: Option<EmployeeLoginSummary>,
    /// Timestamp when this employee profile was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when this employee profile was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version number.
    pub version: i64,
    /// Soft-deletion timestamp, if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// Body for `POST /employees`. `key` is never client-supplied — generated
/// server-side on create only.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateEmployeeRequest {
    pub name: String,
    pub phone: String,
    #[serde(default)]
    pub nic_or_id: Option<String>,
    pub role: EmployeeRole,
    pub default_split_type: SplitType,
    pub default_split_value: f64,
    #[serde(default)]
    pub status: Option<EmployeeStatus>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Body for `PATCH /employees/{id}`. Every field optional so a client sends
/// only what changed — `service::update_employee` fills in omitted fields
/// from the existing document rather than clearing them (same convention as
/// `suppliers::service::update_supplier`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEmployeeRequest {
    pub name: Option<String>,
    pub phone: Option<String>,
    pub nic_or_id: Option<String>,
    pub role: Option<EmployeeRole>,
    pub default_split_type: Option<SplitType>,
    pub default_split_value: Option<f64>,
    pub status: Option<EmployeeStatus>,
    pub notes: Option<String>,
}

/// Query params for `GET /employees`. `search` matches against
/// name/phone/nicOrId.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct EmployeeListQuery {
    pub search: Option<String>,
    pub role: Option<EmployeeRole>,
    pub status: Option<EmployeeStatus>,
}

/// Response for `GET /employees`. Not paginated — a shop's staff roster is
/// small enough to return in full, matching `suppliers`'/`users`' list
/// contract.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeesResponse {
    pub employees: Vec<Employee>,
}

/// Body for `DELETE /employees/batch`. Ids that fail to delete (invalid
/// `ObjectId`, not found, or blocked by the `EMPLOYEE_HAS_LOGIN` guard) are
/// silently skipped rather than failing the whole request — see
/// `service::delete_employees`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct DeleteEmployeesRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteEmployeesResponse {
    pub deleted_count: u64,
}

/// One itemized commission line item for a single employee — the
/// server-computed replacement for the frontend's former mock earnings
/// ledger. Built by `modules::reports::service::employee_earnings`, not
/// persisted anywhere; recomputed from the underlying repair/print-job
/// documents on every request.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeEarningRecord {
    pub work_id: String,
    pub ticket_or_invoice_number: String,
    /// `"repair"` or `"print"`.
    pub work_type: String,
    pub description: String,
    pub customer_name: String,
    pub total_amount_cents: i64,
    pub cost_cents: i64,
    pub profit_cents: i64,
    pub split_type: String,
    pub split_value: f64,
    pub earned_amount_cents: i64,
    /// `"completed"` once the source ticket is delivered, `"pending"` otherwise.
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// Response for `GET /reports/employee-commissions/{employeeKey}/items`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeEarningsResponse {
    pub records: Vec<EmployeeEarningRecord>,
}
