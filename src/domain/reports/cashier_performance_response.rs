use super::cashier_performance_entry::CashierPerformanceEntry;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashierPerformanceResponse {
    pub cashiers: Vec<CashierPerformanceEntry>,
    pub total_revenue_cents: i64,
    pub total_invoices: u64,
}
