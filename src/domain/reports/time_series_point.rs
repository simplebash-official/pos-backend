use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One bucket of the revenue/profit time series.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeSeriesPoint {
    /// Start of the bucket (UTC instant of shop-local bucket start).
    pub period_start: DateTime<Utc>,
    /// Pre-formatted axis label (`"2026-08"`, `"W35"`, `"30 Aug"`, `"2026"`).
    pub label: String,
    pub revenue_cents: i64,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub discount_cents: i64,
    pub cogs_cents: i64,
    pub gross_profit_cents: i64,
    pub gross_margin_bps: i64,
    pub commission_cents: i64,
    pub net_profit_cents: i64,
    pub invoice_count: u64,
}
