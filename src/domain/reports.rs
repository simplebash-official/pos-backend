// Pure business types for the reports and analytics feature — no I/O, no
// Mongo/Axum types beyond serde/utoipa derives. Handlers wrap these in
// `core::response::ApiResponse<T>` before returning.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

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

/// Query parameters for Daily Sales report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DailySalesQuery {
    /// Target date (YYYY-MM-DD), defaults to today.
    pub date: Option<String>,
    /// Optional start date (YYYY-MM-DD) for multi-day daily report.
    pub from: Option<String>,
    /// Optional end date (YYYY-MM-DD) for multi-day daily report.
    pub to: Option<String>,
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

/// Daily sales summary for a single calendar day.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailySalesReportSummary {
    /// Date in YYYY-MM-DD format.
    pub date: String,
    /// Total gross sales before discounts in cents.
    pub gross_sales_cents: i64,
    /// Total discounts given in cents.
    pub discount_cents: i64,
    /// Total net sales in cents (gross - discount).
    pub total_sales_cents: i64,
    /// Total number of invoices issued on this date.
    pub total_invoices: u64,
    /// Revenue from repair service jobs in cents.
    pub repair_revenue_cents: i64,
    /// Revenue from print jobs in cents.
    pub print_revenue_cents: i64,
    /// Revenue from retail merchandise in cents.
    pub retail_revenue_cents: i64,
    /// Payment method distributions on this date.
    pub payment_methods: PaymentMethodBreakdown,
}

/// Daily Sales report payload with daily entries and aggregate totals.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DailySalesReportResponse {
    pub summaries: Vec<DailySalesReportSummary>,
    pub total_sales_cents: i64,
    pub total_invoices: u64,
    pub total_repair_revenue_cents: i64,
    pub total_print_revenue_cents: i64,
    pub total_retail_revenue_cents: i64,
    pub total_discounts_cents: i64,
    pub payment_methods: PaymentMethodBreakdown,
}

/// Query parameters for Monthly Profit report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyProfitQuery {
    /// Month in YYYY-MM format (e.g. "2026-08"), defaults to current month.
    pub month: Option<String>,
    /// Year in YYYY format (e.g. "2026"), defaults to current year.
    pub year: Option<i32>,
}

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

/// Monthly Profit and Financial Intelligence report response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyProfitReportResponse {
    /// Year and month in YYYY-MM format.
    pub year_month: String,
    pub total_revenue_cents: i64,
    pub retail_revenue_cents: i64,
    pub repair_revenue_cents: i64,
    pub print_revenue_cents: i64,
    pub total_discounts_cents: i64,
    pub cogs_cents: i64,
    pub gross_profit_cents: i64,
    pub commission_payouts_cents: i64,
    pub net_profit_cents: i64,
    pub total_refunds_cents: i64,
    pub total_invoices_count: u64,
    pub daily_breakdown: Vec<MonthlyProfitDayEntry>,
}

/// Query parameters for Outstanding Receivables report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct OutstandingReceivablesQuery {
    /// Optional customer name, phone, or invoice number search filter.
    pub search: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

/// One credit invoice with unpaid balance.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutstandingInvoiceEntry {
    pub id: String,
    pub key: String,
    pub invoice_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    pub total_cents: i64,
    pub amount_paid_cents: i64,
    pub balance_due_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    pub is_overdue: bool,
    pub created_at: DateTime<Utc>,
}

/// Outstanding receivables overview and paginated unpaid invoices.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutstandingReceivablesResponse {
    pub total_outstanding_cents: i64,
    pub total_credit_invoices_count: u64,
    pub overdue_invoices_count: u64,
    pub overdue_amount_cents: i64,
    pub invoices: Vec<OutstandingInvoiceEntry>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}

/// Query parameters for Employee Commissions report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeCommissionsQuery {
    pub preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    /// Filters to one employee by key. Grouping is by key, not display name
    /// — two employees sharing a name never merge, and a rename doesn't
    /// split history across two buckets (see
    /// `reports::repository::commissions::aggregate_employee_commissions`).
    pub employee_key: Option<String>,
}

/// Performance and commission breakdown for one employee/technician.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeePerformanceEntry {
    /// `None` for the "unassigned" bucket — there is no real employee to key by.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee_key: Option<String>,
    pub employee_name: String,
    pub role: String,
    pub assigned_jobs_count: u64,
    pub revenue_generated_cents: i64,
    pub estimated_cost_cents: i64,
    pub profit_cents: i64,
    pub earned_commission_cents: i64,
    pub net_shop_contribution_cents: i64,
}

/// Employee Commission and Performance report payload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeCommissionsReportResponse {
    pub employees: Vec<EmployeePerformanceEntry>,
    pub total_commissions_cents: i64,
    pub total_revenue_cents: i64,
    pub total_jobs_count: u64,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

/// Valuation and stock breakdown for one category.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryValuationEntry {
    pub category_key: String,
    pub category_name: String,
    pub product_count: u64,
    pub total_units: i64,
    pub cost_valuation_cents: i64,
    pub retail_valuation_cents: i64,
    pub potential_profit_cents: i64,
}

/// Full inventory valuation report payload.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InventoryValuationResponse {
    pub total_products_count: u64,
    pub total_stock_units: i64,
    pub total_cost_valuation_cents: i64,
    pub total_retail_valuation_cents: i64,
    pub potential_gross_profit_cents: i64,
    pub potential_margin_percentage: f64,
    pub low_stock_products_count: u64,
    pub out_of_stock_products_count: u64,
    pub categories: Vec<CategoryValuationEntry>,
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

/// Top Products report response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopProductsResponse {
    pub products: Vec<TopProductEntry>,
    pub total_units_sold: i64,
    pub total_revenue_cents: i64,
}

// ============================================================================
// Analytics & Reports — `/reports/analytics/*`
//
// Shop-local (Asia/Colombo, UTC+05:30) date handling via
// `service::dates::parse_date_range_tz`; time buckets via Mongo `$dateTrunc`.
// Every response is camelCase, money is integer cents. `*Bps` fields are
// basis points (1% = 100 bps) so ratios stay integer.
// ============================================================================

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

/// A single time-bucket granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Day,
    Week,
    Month,
    Year,
}

impl Granularity {
    pub fn as_mongo_unit(self) -> &'static str {
        match self {
            Granularity::Day => "day",
            Granularity::Week => "week",
            Granularity::Month => "month",
            Granularity::Year => "year",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Granularity::Day => "Daily",
            Granularity::Week => "Weekly",
            Granularity::Month => "Monthly",
            Granularity::Year => "Yearly",
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopCustomersResponse {
    pub customers: Vec<TopCustomerEntry>,
    pub total_customers: u64,
    pub total_revenue_cents: i64,
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SalesByCategoryResponse {
    pub rows: Vec<CategorySalesRow>,
    /// Retail lines whose product no longer resolves (deleted) land here.
    pub uncategorised: CategorySalesRow,
    pub total_revenue_cents: i64,
    pub total_gross_profit_cents: i64,
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

// --- sales patterns (weekday x hour) -----------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PatternCell {
    /// 1 = Monday … 7 = Sunday (`$isoDayOfWeek`).
    pub weekday: u8,
    /// 0–23, shop-local.
    pub hour: u8,
    pub invoice_count: u64,
    pub revenue_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PatternBucket {
    pub bucket: u8,
    pub label: String,
    pub invoice_count: u64,
    pub revenue_cents: i64,
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

// --- receivables aging -------------------------------------------------

/// Query for `GET /reports/analytics/receivables-aging`.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ReceivablesAgingQuery {
    /// Snapshot date (YYYY-MM-DD), defaults to today.
    pub as_of: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgingBucket {
    pub label: String,
    pub min_days: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_days: Option<i32>,
    pub invoice_count: u64,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgingDebtor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    pub customer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone: Option<String>,
    pub outstanding_cents: i64,
    pub oldest_days: i32,
    pub invoice_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceivablesAgingResponse {
    pub as_of: DateTime<Utc>,
    pub buckets: Vec<AgingBucket>,
    pub total_outstanding_cents: i64,
    pub total_invoices: u64,
    /// Customers with the largest 60+ day exposure (up to 10).
    pub top_debtors: Vec<AgingDebtor>,
}

// --- discounts --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscountTypeRow {
    /// `percentage` | `fixed` | `none`.
    pub discount_type: String,
    pub invoice_count: u64,
    pub discount_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscountCashierRow {
    pub cashier_id: String,
    pub cashier_name: String,
    pub discount_cents: i64,
    pub revenue_cents: i64,
    pub discount_rate_bps: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscountProductRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    pub name: String,
    pub discount_cents: i64,
    pub units_sold: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscountAnalyticsResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_discount_cents: i64,
    pub gross_before_discount_cents: i64,
    pub discount_rate_bps: i64,
    pub invoices_with_discount: u64,
    pub invoice_count: u64,
    pub order_level_discount_cents: i64,
    pub line_level_discount_cents: i64,
    pub by_type: Vec<DiscountTypeRow>,
    pub by_cashier: Vec<DiscountCashierRow>,
    pub top_discounted_products: Vec<DiscountProductRow>,
}

// --- refunds ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundReasonRow {
    pub reason: String,
    pub credit_note_item_count: u64,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundMethodRow {
    pub method: String,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundProductRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    pub name: String,
    pub quantity: i64,
    pub amount_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundAnalyticsResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub credit_note_count: u64,
    pub net_refund_cents: i64,
    pub refund_cash_cents: i64,
    pub balance_reduction_cents: i64,
    /// Net refunds as a share of period revenue, in basis points.
    pub refund_rate_bps: i64,
    pub exchange_count: u64,
    pub no_receipt_count: u64,
    pub manager_override_count: u64,
    pub by_reason: Vec<RefundReasonRow>,
    pub by_method: Vec<RefundMethodRow>,
    pub top_returned_products: Vec<RefundProductRow>,
}
