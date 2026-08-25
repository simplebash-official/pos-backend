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
