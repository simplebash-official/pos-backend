//! Analytics KPIs, time series, sales breakdowns and top-N lists.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

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

// --- payment methods -------------------------------------------------------

/// `GET /reports/analytics/payment-methods` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsPaymentMethodsResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub breakdown: PaymentMethodBreakdown,
    pub total_cents: i64,
}

// Analytics & Reports — `/reports/analytics/*`
//
// Shop-local (Asia/Colombo, UTC+05:30) date handling via
// `service::dates::parse_date_range_tz`; time buckets via Mongo `$dateTrunc`.
// Every response is camelCase, money is integer cents. `*Bps` fields are
// basis points (1% = 100 bps) so ratios stay integer.

/// Shared query for the analytics endpoints. Presets match
/// `service::dates::parse_date_range` (`today` … `custom`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsRangeQuery {
    /// Preset range, or `"custom"` with `from`/`to`.
    pub preset: Option<String>,
    /// Start (YYYY-MM-DD in shop-local time, or RFC3339), used for `"custom"`.
    pub from: Option<String>,
    /// End (YYYY-MM-DD in shop-local time, or RFC3339), used for `"custom"`.
    pub to: Option<String>,
    /// Time-bucket size for series endpoints: `day` | `week` | `month` | `year`.
    /// Omitted → chosen automatically from the range span.
    pub granularity: Option<String>,
    /// When true, also compute the immediately-preceding equal-length window
    /// and per-metric deltas.
    pub compare_previous: Option<bool>,
}

/// `GET /reports/analytics/summary` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsSummaryResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub current: AnalyticsKpis,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<AnalyticsKpis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deltas: Option<AnalyticsKpiDeltas>,
}

// --- cashier performance ------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashierPerformanceEntry {
    pub cashier_id: String,
    pub cashier_name: String,
    pub invoice_count: u64,
    pub revenue_cents: i64,
    pub retail_revenue_cents: i64,
    pub items_sold: i64,
    pub discount_given_cents: i64,
    pub discount_rate_bps: i64,
    pub avg_basket_cents: i64,
    pub credit_invoice_count: u64,
    pub refund_count: u64,
    pub refunded_cents: i64,
    pub gross_profit_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashierPerformanceResponse {
    pub cashiers: Vec<CashierPerformanceEntry>,
    pub total_revenue_cents: i64,
    pub total_invoices: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategorySalesRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_key: Option<String>,
    pub category_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcategory_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subcategory_name: Option<String>,
    pub units_sold: i64,
    pub revenue_cents: i64,
    pub discount_cents: i64,
    pub cogs_cents: i64,
    pub gross_profit_cents: i64,
    pub gross_margin_bps: i64,
}

/// A single time-bucket granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Day,
    Week,
    Month,
    Year,
}

impl Granularity {
    #[must_use]
    /// The MongoDB `$dateTrunc` unit for this granularity.
    pub fn as_mongo_unit(self) -> &'static str {
        match self {
            Granularity::Day => "day",
            Granularity::Week => "week",
            Granularity::Month => "month",
            Granularity::Year => "year",
        }
    }

    #[must_use]
    /// The label shown for this granularity.
    pub fn label(self) -> &'static str {
        match self {
            Granularity::Day => "Daily",
            Granularity::Week => "Weekly",
            Granularity::Month => "Monthly",
            Granularity::Year => "Yearly",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
/// Sales in one weekday or hour of the sales-pattern report.
pub struct PatternBucket {
    pub bucket: u8,
    pub label: String,
    pub invoice_count: u64,
    pub revenue_cents: i64,
}

// --- sales patterns (weekday x hour) -----------------------------------
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
/// Sales in one weekday-and-hour cell of the sales-pattern report.
pub struct PatternCell {
    /// 1 = Monday … 7 = Sunday (`$isoDayOfWeek`).
    pub weekday: u8,
    /// 0–23, shop-local.
    pub hour: u8,
    pub invoice_count: u64,
    pub revenue_cents: i64,
}

/// Payment method distribution.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentMethodBreakdown {
    pub cash_cents: i64,
    pub card_cents: i64,
    pub online_cents: i64,
    pub credit_cents: i64,
}

/// Query parameters for date-filtered reports with preset or custom range.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ReportDateRangeQuery {
    /// Preset range: `"today"`, `"yesterday"`, `"this_week"`, `"this_month"`,
    /// `"last_month"`, `"this_year"`, `"all_time"`, or `"custom"`.
    pub preset: Option<String>,
    /// Start date (YYYY-MM-DD or ISO 8601 timestamp), used for `"custom"`.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD or ISO 8601 timestamp), used for `"custom"`.
    pub to: Option<String>,
}

// --- sales by category ---------------------------------------------------

/// Query for `GET /reports/analytics/sales-by-category`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct SalesByCategoryQuery {
    pub preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// `category` (default) | `subcategory`.
    pub group_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesByCategoryResponse {
    pub rows: Vec<CategorySalesRow>,
    /// Retail lines whose product no longer resolves (deleted) land here.
    pub uncategorised: CategorySalesRow,
    pub total_revenue_cents: i64,
    pub total_gross_profit_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesPatternsResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    /// 7 × 24 = 168 cells, zero-filled, row-major (weekday then hour).
    pub cells: Vec<PatternCell>,
    pub by_weekday: Vec<PatternBucket>,
    pub by_hour: Vec<PatternBucket>,
    pub busiest: PatternCell,
}

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

/// `GET /reports/analytics/timeseries` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeSeriesResponse {
    /// Resolved granularity (`day` | `week` | `month` | `year`).
    pub granularity: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    /// Continuous, gap-filled buckets in ascending order.
    pub points: Vec<TimeSeriesPoint>,
    /// Sum across all buckets.
    pub totals: TimeSeriesPoint,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopCustomerEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    /// True for the single collapsed "Walk-in" (no linked customer) bucket.
    pub is_walk_in: bool,
    pub invoice_count: u64,
    pub revenue_cents: i64,
    pub discount_cents: i64,
    pub gross_profit_cents: i64,
    /// Current all-time balance owed by this customer (from the customer record).
    pub outstanding_cents: i64,
    pub last_purchase_at: DateTime<Utc>,
}

// --- top customers -------------------------------------------------------

/// Query for `GET /reports/analytics/top-customers`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct TopCustomersQuery {
    pub preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// Max rows, 1–100 (default 20).
    pub limit: Option<u64>,
    /// `revenue` (default) | `invoices` | `profit`.
    pub sort_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopCustomersResponse {
    pub customers: Vec<TopCustomerEntry>,
    pub total_customers: u64,
    pub total_revenue_cents: i64,
}

/// Line item for a ranked top-selling product.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopProductEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    pub source_type: String,
    pub units_sold: i64,
    pub total_revenue_cents: i64,
    pub total_discount_cents: i64,
}

/// Query parameters for Top Products report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct TopProductsQuery {
    pub preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<u64>,
    /// Sort criteria: `"revenue"` or `"quantity"`, defaults to `"revenue"`.
    pub sort_by: Option<String>,
}

/// Top Products report response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopProductsResponse {
    pub products: Vec<TopProductEntry>,
    pub total_units_sold: i64,
    pub total_revenue_cents: i64,
}
