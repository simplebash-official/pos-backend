// MongoDB aggregation queries for sales and revenue analysis over the `invoices` collection.

mod aggregate_daily_sales;
mod aggregate_sales_summary;
mod aggregate_top_products;
mod compute_payment_methods;
mod get_i64_flexible;
mod invoices;
mod sales_summary_aggregate;

pub(crate) use aggregate_daily_sales::aggregate_daily_sales;
pub(crate) use aggregate_sales_summary::aggregate_sales_summary;
pub(crate) use aggregate_top_products::aggregate_top_products;
pub(crate) use compute_payment_methods::compute_payment_methods;
pub(crate) use get_i64_flexible::get_i64_flexible;
