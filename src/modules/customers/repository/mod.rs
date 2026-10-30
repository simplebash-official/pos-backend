// Dual-engine (Mongo / SQLite) access for the `customers` collection.
// Functions here return `Option`/`Vec`/counts straight from the driver.

mod adjust_customer_financials;
mod aggregate_stats;
mod customer_sqlite_row;
mod customers;
mod delete_customer;
mod distinct_tags;
mod find_customer_by_id;
mod find_customer_by_id_or_key;
mod find_customer_by_key;
mod find_customers_by_keys;
mod insert_customer;
mod list_customers;
mod stats_aggregate_result;
mod update_customer;

pub(crate) use adjust_customer_financials::adjust_customer_financials;
pub(crate) use aggregate_stats::aggregate_stats;
pub(crate) use delete_customer::delete_customer;
pub(crate) use distinct_tags::distinct_tags;

pub(crate) use find_customer_by_id_or_key::find_customer_by_id_or_key;
pub(crate) use find_customer_by_key::find_customer_by_key;

pub(crate) use insert_customer::insert_customer;
pub(crate) use list_customers::list_customers;

pub(crate) use update_customer::update_customer;
