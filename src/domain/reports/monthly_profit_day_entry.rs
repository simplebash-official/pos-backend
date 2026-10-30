use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Single day line item within the monthly profit report.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyProfitDayEntry {
    pub date: String,
    pub revenue_cents: i64,
    pub cogs_cents: i64,
    pub gross_profit_cents: i64,
    pub commissions_cents: i64,
    pub net_profit_cents: i64,
    pub invoice_count: u64,
}
