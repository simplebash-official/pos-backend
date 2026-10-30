use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Performance and commission breakdown for one employee/technician.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeePerformanceEntry {
    /// `None` for the "unassigned" bucket — there is no real employee to key by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee_key: Option<String>,
    pub employee_name: String,
    pub role: String,
    pub assigned_jobs_count: u64,
    pub revenue_generated_cents: i64,
    pub estimated_cost_cents: i64,
    pub profit_cents: i64,
    pub earned_commission_cents: i64,
    pub net_shop_contribution_cents: i64,
}
