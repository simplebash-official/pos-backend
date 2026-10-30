use super::employee_earning_record::EmployeeEarningRecord;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /reports/employee-commissions/{employeeKey}/items`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeEarningsResponse {
    pub records: Vec<EmployeeEarningRecord>,
}
