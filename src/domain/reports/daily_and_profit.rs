//! Daily sales, monthly profit, commissions and inventory valuation reports.

use super::analytics::PaymentMethodBreakdown;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

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

/// Query parameters for Monthly Profit report.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyProfitQuery {
    /// Month in YYYY-MM format (e.g. "2026-08"), defaults to current month.
    pub month: Option<String>,
    /// Year in YYYY format (e.g. "2026"), defaults to current year.
    pub year: Option<i32>,
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
