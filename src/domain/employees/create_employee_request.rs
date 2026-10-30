use super::employee_role::EmployeeRole;
use super::employee_status::EmployeeStatus;
use super::split_type::SplitType;

use serde::Deserialize;
use utoipa::ToSchema;

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
