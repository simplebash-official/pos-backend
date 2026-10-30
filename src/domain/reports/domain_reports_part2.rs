use super::domain_reports_part1::{
    CashierPerformanceResponse, DailySalesReportResponse, DiscountAnalyticsResponse,
    EmployeeCommissionsReportResponse, PatternBucket, PatternCell, RefundAnalyticsResponse,
    SalesByCategoryResponse,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Sales section feed bundle.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesFeedData {
    pub daily_sales: DailySalesReportResponse,
    pub sales_by_category: SalesByCategoryResponse,
    pub discounts: DiscountAnalyticsResponse,
    pub refunds: RefundAnalyticsResponse,
    pub top_products: TopProductsResponse,
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

/// Staff section feed bundle.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffFeedData {
    pub employee_commissions: EmployeeCommissionsReportResponse,
    pub cashier_performance: CashierPerformanceResponse,
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
