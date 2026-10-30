use super::employee_performance_entry::EmployeePerformanceEntry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Employee Commission and Performance report payload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeCommissionsReportResponse {
    pub employees: Vec<EmployeePerformanceEntry>,
    pub total_commissions_cents: i64,
    pub total_revenue_cents: i64,
    pub total_jobs_count: u64,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}
