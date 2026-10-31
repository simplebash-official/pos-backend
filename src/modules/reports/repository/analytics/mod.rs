//! MongoDB aggregation for the Analytics & Reports endpoints (`/reports/analytics/*`).
//!
//! All pipelines `$match` `invoices` on `{ status: { $ne: "voided" }, created_at
//! in [start, end) }` and, where a line-level figure is needed, `$unwind
//! "$items"` while `$ifNull`-coalescing the two historical field spellings
//! (`sourceType`/`source_type`, `totalCents`/`total_cents`,
//! `unitCostCents`/`unit_cost_cents`, …) exactly as `repository::sales` does.
//!
//! Time buckets use `$dateTrunc` with `timezone: "+05:30"` (Sri Lanka is
//! permanently UTC+05:30) and `startOfWeek: "monday"` so a bucket lines up
//! with the shop owner's local calendar — see
//! `service::dates::{REPORT_TZ_MONGO, parse_date_range_tz}`.

mod analytics_shared;
mod cashier_analytics;
mod receivables_analytics;
mod refund_discount_analytics;
mod revenue_analytics;
mod sales_breakdown_analytics;

pub(crate) use cashier_analytics::aggregate_cashier_performance;
pub(crate) use cashier_analytics::aggregate_commission_series;
pub(crate) use receivables_analytics::aggregate_receivables_aging;
pub(crate) use refund_discount_analytics::aggregate_cashier_refunds;
pub(crate) use refund_discount_analytics::aggregate_discounts;
pub(crate) use refund_discount_analytics::aggregate_refund_detail;
pub(crate) use revenue_analytics::aggregate_invoice_kpis;
pub(crate) use revenue_analytics::aggregate_payment_methods;
pub(crate) use revenue_analytics::aggregate_revenue_series;
pub(crate) use sales_breakdown_analytics::aggregate_sales_by_category;
pub(crate) use sales_breakdown_analytics::aggregate_sales_patterns;
pub(crate) use sales_breakdown_analytics::aggregate_top_customers;

pub(crate) use receivables_analytics::fetch_customer_balances;
