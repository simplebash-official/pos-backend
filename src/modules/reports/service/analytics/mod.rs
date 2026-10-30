// Orchestration for the Analytics & Reports endpoints. Resolves the
// shop-local date range and bucket granularity, fans out to the
// `repository::analytics` pipelines plus the existing commission/refund
// aggregations, and assembles the gap-filled, derived-figure responses the
// `/reports/analytics/*` handlers return.

mod bps;
mod get_analytics_cashier_performance;
mod get_analytics_discounts;
mod get_analytics_payment_methods;
mod get_analytics_receivables_aging;
mod get_analytics_refunds;
mod get_analytics_sales_by_category;
mod get_analytics_sales_patterns;
mod get_analytics_summary;
mod get_analytics_timeseries;
mod get_analytics_top_customers;
mod range;

pub(crate) use get_analytics_cashier_performance::get_analytics_cashier_performance;
pub(crate) use get_analytics_discounts::get_analytics_discounts;
pub(crate) use get_analytics_payment_methods::get_analytics_payment_methods;
pub(crate) use get_analytics_receivables_aging::get_analytics_receivables_aging;
pub(crate) use get_analytics_refunds::get_analytics_refunds;
pub(crate) use get_analytics_sales_by_category::get_analytics_sales_by_category;
pub(crate) use get_analytics_sales_patterns::get_analytics_sales_patterns;
pub(crate) use get_analytics_summary::get_analytics_summary;
pub(crate) use get_analytics_timeseries::get_analytics_timeseries;
pub(crate) use get_analytics_top_customers::get_analytics_top_customers;
