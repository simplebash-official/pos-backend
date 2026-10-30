use crate::domain::reports::TimeSeriesPoint;

/// Enriched time series point with cumulative metrics and moving averages.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnrichedTimeSeriesPoint {
    #[serde(flatten)]
    pub point: TimeSeriesPoint,
    pub cumulative_revenue_cents: i64,
    pub cumulative_profit_cents: i64,
    pub moving_avg_revenue_cents: Option<i64>,
}
