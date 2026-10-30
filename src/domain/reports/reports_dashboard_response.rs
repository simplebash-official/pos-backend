use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Executive overview & KPI metrics for the Reports & Profit Intelligence dashboard.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportsDashboardResponse {
    /// Total net sales revenue in cents across all valid invoices in the period.
    pub total_revenue_cents: i64,
    /// Revenue generated from retail product sales in cents.
    pub retail_revenue_cents: i64,
    /// Revenue generated from repair services in cents.
    pub repair_revenue_cents: i64,
    /// Revenue generated from print jobs in cents.
    pub print_revenue_cents: i64,
    /// Count of completed invoices in the period.
    pub total_invoices_count: u64,
    /// Total discount amount granted in cents.
    pub total_discounts_cents: i64,
    /// Estimated cost of goods sold and service materials in cents.
    pub estimated_cogs_cents: i64,
    /// Estimated gross profit (total_revenue - estimated_cogs) in cents.
    pub estimated_gross_profit_cents: i64,
    /// Total employee profit-split commission payouts in cents.
    pub total_commissions_cents: i64,
    /// Net shop profit (gross_profit - total_commissions) in cents.
    pub net_shop_profit_cents: i64,
    /// Total uncollected credit balance across all pending/partially paid credit invoices (all-time).
    pub outstanding_credit_cents: i64,
    /// Start of the evaluated period (UTC).
    pub period_start: DateTime<Utc>,
    /// End of the evaluated period (UTC).
    pub period_end: DateTime<Utc>,
}
