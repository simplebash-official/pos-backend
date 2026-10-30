use super::employee_login_summary::EmployeeLoginSummary;
use super::employee_role::EmployeeRole;
use super::employee_status::EmployeeStatus;
use super::split_type::SplitType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
