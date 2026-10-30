//! MongoDB aggregation queries for sales and revenue analysis over the `invoices` collection.

mod repository_sales;

pub(crate) use repository_sales::aggregate_daily_sales;
pub(crate) use repository_sales::aggregate_sales_summary;
pub(crate) use repository_sales::aggregate_top_products;
pub(crate) use repository_sales::compute_payment_methods;
pub(crate) use repository_sales::get_i64_flexible;
