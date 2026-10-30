use super::employee::Employee;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /employees`. Not paginated — a shop's staff roster is
/// small enough to return in full, matching `suppliers`'/`users`' list
/// contract.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeesResponse {
    pub employees: Vec<Employee>,
}
