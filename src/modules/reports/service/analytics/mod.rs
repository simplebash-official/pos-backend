//! Orchestration for the Analytics & Reports endpoints. Resolves the
//! shop-local date range and bucket granularity, fans out to the
//! `repository::analytics` pipelines plus the existing commission/refund
//! aggregations, and assembles the gap-filled, derived-figure responses the
//! `/reports/analytics/*` handlers return.

mod service_analytics;

pub(crate) use service_analytics::get_analytics_cashier_performance;
pub(crate) use service_analytics::get_analytics_discounts;
pub(crate) use service_analytics::get_analytics_payment_methods;
pub(crate) use service_analytics::get_analytics_receivables_aging;
pub(crate) use service_analytics::get_analytics_refunds;
pub(crate) use service_analytics::get_analytics_sales_by_category;
pub(crate) use service_analytics::get_analytics_sales_patterns;
pub(crate) use service_analytics::get_analytics_summary;
pub(crate) use service_analytics::get_analytics_timeseries;
pub(crate) use service_analytics::get_analytics_top_customers;
