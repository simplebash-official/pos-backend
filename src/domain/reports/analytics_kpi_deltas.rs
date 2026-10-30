use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Per-metric change from the previous period, in basis points. `None` when
/// the previous value is zero (no meaningful ratio).
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsKpiDeltas {
    pub total_revenue_bps: Option<i64>,
    pub gross_profit_bps: Option<i64>,
    pub net_profit_bps: Option<i64>,
    pub invoice_count_bps: Option<i64>,
    pub avg_basket_bps: Option<i64>,
    /// Absolute change in gross margin, in basis points (not a ratio).
    pub gross_margin_delta_bps: i64,
}
