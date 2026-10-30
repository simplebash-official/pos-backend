use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Headline KPIs for one period.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsKpis {
    pub total_revenue_cents: i64,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub invoice_count: u64,
    pub items_sold: i64,
    pub avg_basket_cents: i64,
    pub discount_cents: i64,
    /// Discount as a share of gross (pre-discount) sales, in basis points.
    pub discount_rate_bps: i64,
    /// Retail cost of goods sold — `Σ unitCostCents · quantity` over retail
    /// lines that carry a cost snapshot.
    pub retail_cogs_cents: i64,
    /// Repair/print material cost recognised in the period.
    pub service_material_cost_cents: i64,
    pub gross_profit_cents: i64,
    /// Gross margin as a share of revenue, in basis points.
    pub gross_margin_bps: i64,
    pub commission_payouts_cents: i64,
    /// Value credited back to customers via non-voided credit notes.
    pub refunds_cents: i64,
    /// Refunds as a share of revenue, in basis points.
    pub refund_rate_bps: i64,
    pub net_profit_cents: i64,
    /// Retail revenue whose lines carried a known cost, as a share of all
    /// retail revenue, in basis points — 10000 means every retail line had a
    /// cost snapshot and the profit figures are exact.
    pub cogs_coverage_bps: i64,
}
