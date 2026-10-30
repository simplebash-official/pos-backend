use super::employee_role::EmployeeRole;
use super::employee_status::EmployeeStatus;
use super::split_type::SplitType;

use serde::Deserialize;
use utoipa::ToSchema;

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
