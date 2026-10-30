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

mod repository_analytics_part1;
mod repository_analytics_part2;
mod repository_analytics_part3;

pub(crate) use repository_analytics_part1::aggregate_cashier_performance;
pub(crate) use repository_analytics_part1::aggregate_cashier_refunds;
pub(crate) use repository_analytics_part1::aggregate_commission_series;
pub(crate) use repository_analytics_part1::aggregate_discounts;
pub(crate) use repository_analytics_part1::aggregate_invoice_kpis;
pub(crate) use repository_analytics_part1::aggregate_payment_methods;
pub(crate) use repository_analytics_part1::aggregate_receivables_aging;
pub(crate) use repository_analytics_part2::aggregate_refund_detail;
pub(crate) use repository_analytics_part2::aggregate_revenue_series;
pub(crate) use repository_analytics_part2::aggregate_sales_by_category;
pub(crate) use repository_analytics_part2::aggregate_sales_patterns;
pub(crate) use repository_analytics_part2::aggregate_top_customers;

pub(crate) use repository_analytics_part3::fetch_customer_balances;
