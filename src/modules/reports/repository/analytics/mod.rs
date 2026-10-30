// MongoDB aggregation for the Analytics & Reports endpoints (`/reports/analytics/*`).
//
// All pipelines `$match` `invoices` on `{ status: { $ne: "voided" }, created_at
// in [start, end) }` and, where a line-level figure is needed, `$unwind
// "$items"` while `$ifNull`-coalescing the two historical field spellings
// (`sourceType`/`source_type`, `totalCents`/`total_cents`,
// `unitCostCents`/`unit_cost_cents`, …) exactly as `repository::sales` does.
//
// Time buckets use `$dateTrunc` with `timezone: "+05:30"` (Sri Lanka is
// permanently UTC+05:30) and `startOfWeek: "monday"` so a bucket lines up
// with the shop owner's local calendar — see
// `service::dates::{REPORT_TZ_MONGO, parse_date_range_tz}`.

mod aggregate_cashier_performance;
mod aggregate_cashier_refunds;
mod aggregate_commission_series;
mod aggregate_discounts;
mod aggregate_invoice_kpis;
mod aggregate_payment_methods;
mod aggregate_receivables_aging;
mod aggregate_refund_detail;
mod aggregate_revenue_series;
mod aggregate_sales_by_category;
mod aggregate_sales_patterns;
mod aggregate_top_customers;
mod aging_invoice_row;
mod align_bucket_instant;
mod cashier_agg_row;
mod category_agg_row;
mod date_match;
mod discount_agg;
mod fetch_customer_balances;
mod invoice_kpi_aggregate;
mod invoices;
mod raw_series_bucket;
mod refund_detail_agg;
mod top_customer_agg_row;
mod trunc;

pub(crate) use aggregate_cashier_performance::aggregate_cashier_performance;
pub(crate) use aggregate_cashier_refunds::aggregate_cashier_refunds;
pub(crate) use aggregate_commission_series::aggregate_commission_series;
pub(crate) use aggregate_discounts::aggregate_discounts;
pub(crate) use aggregate_invoice_kpis::aggregate_invoice_kpis;
pub(crate) use aggregate_payment_methods::aggregate_payment_methods;
pub(crate) use aggregate_receivables_aging::aggregate_receivables_aging;
pub(crate) use aggregate_refund_detail::aggregate_refund_detail;
pub(crate) use aggregate_revenue_series::aggregate_revenue_series;
pub(crate) use aggregate_sales_by_category::aggregate_sales_by_category;
pub(crate) use aggregate_sales_patterns::aggregate_sales_patterns;
pub(crate) use aggregate_top_customers::aggregate_top_customers;

pub(crate) use fetch_customer_balances::fetch_customer_balances;
