use super::employee_role::EmployeeRole;
use super::employee_status::EmployeeStatus;

use serde::Deserialize;
use utoipa::IntoParams;

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
