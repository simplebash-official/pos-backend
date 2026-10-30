use super::cashier_performance_response::CashierPerformanceResponse;
use super::employee_commissions_report_response::EmployeeCommissionsReportResponse;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Staff section feed bundle.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffFeedData {
    pub employee_commissions: EmployeeCommissionsReportResponse,
    pub cashier_performance: CashierPerformanceResponse,
}
