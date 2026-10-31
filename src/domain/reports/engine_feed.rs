//! The analytics engine's feed, metadata and invalidation payloads.

use super::analytics::AnalyticsPaymentMethodsResponse;
use super::analytics::AnalyticsSummaryResponse;
use super::analytics::TopProductsResponse;
use super::analytics::{
    CashierPerformanceResponse, SalesByCategoryResponse, SalesPatternsResponse, TimeSeriesResponse,
    TopCustomersResponse,
};
use super::daily_and_profit::MonthlyProfitReportResponse;
use super::daily_and_profit::{DailySalesReportResponse, EmployeeCommissionsReportResponse};
use super::receivables::ReceivablesAgingResponse;
use super::refunds_discounts::{DiscountAnalyticsResponse, RefundAnalyticsResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Customers section feed bundle.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomersFeedData {
    pub top_customers: TopCustomersResponse,
    pub receivables_aging: ReceivablesAgingResponse,
}

/// Query parameters for the unified Analytics & Chart Engine feed endpoint.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct EngineFeedQuery {
    /// Section requested: `"overview"`, `"sales"`, `"profit"`, `"customers"`, `"staff"`, or `"all"`. Defaults to `"overview"`.
    pub section: Option<String>,
    /// Date preset: `"today"`, `"yesterday"`, `"this_week"`, `"this_month"`, `"last_month"`, `"this_year"`, or `"custom"`.
    pub preset: Option<String>,
    /// Start date (YYYY-MM-DD or RFC3339 timestamp) for custom ranges.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD or RFC3339 timestamp) for custom ranges.
    pub to: Option<String>,
    /// Granularity: `"day"`, `"week"`, `"month"`, or `"year"`.
    pub granularity: Option<String>,
    /// When true, compute previous period comparison and deltas.
    pub compare_previous: Option<bool>,
    /// Limit for ranked lists (top products, customers). Defaults to 20.
    pub limit: Option<u64>,
    /// Sort criteria for top customers/products (`"revenue"`, `"invoices"`, `"profit"`, `"quantity"`).
    pub sort_by: Option<String>,
    /// Grouping for category sales (`"category"` or `"subcategory"`).
    pub group_by: Option<String>,
    /// Force bypass cache (admin debugging).
    pub bypass_cache: Option<bool>,
}

/// Unified response payload for the Engine Feed.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EngineFeedResponse {
    pub section: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub meta: EngineMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<OverviewFeedData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sales: Option<SalesFeedData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profit: Option<ProfitFeedData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customers: Option<CustomersFeedData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staff: Option<StaffFeedData>,
}

/// Engine cache invalidation response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EngineInvalidateResponse {
    pub invalidated_keys_count: usize,
    pub message: String,
}

/// Metadata describing engine query execution and caching status.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EngineMetadata {
    /// Time taken to process the request on the server in milliseconds.
    pub execution_time_ms: u64,
    /// Whether the response was served from the in-memory cache.
    pub cache_hit: bool,
    /// The timestamp when the data was computed / cached (UTC).
    pub cached_at: DateTime<Utc>,
    /// Cache TTL remaining or status description.
    pub cache_status: String,
}

/// Overview section feed bundle.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OverviewFeedData {
    pub summary: AnalyticsSummaryResponse,
    pub timeseries: TimeSeriesResponse,
    pub payment_methods: AnalyticsPaymentMethodsResponse,
    pub sales_patterns: SalesPatternsResponse,
}

/// Profit section feed bundle.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfitFeedData {
    pub monthly_profit: MonthlyProfitReportResponse,
    pub timeseries: TimeSeriesResponse,
}

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

/// Staff section feed bundle.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffFeedData {
    pub employee_commissions: EmployeeCommissionsReportResponse,
    pub cashier_performance: CashierPerformanceResponse,
}
