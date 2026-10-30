//! Dual-engine (Mongo / SQLite) access for the `customers` collection.
//! Functions here return `Option`/`Vec`/counts straight from the driver.

mod customers_repository;

pub(crate) use customers_repository::adjust_customer_financials;
pub(crate) use customers_repository::aggregate_stats;
pub(crate) use customers_repository::delete_customer;
pub(crate) use customers_repository::distinct_tags;

pub(crate) use customers_repository::find_customer_by_id_or_key;
pub(crate) use customers_repository::find_customer_by_key;

pub(crate) use customers_repository::insert_customer;
pub(crate) use customers_repository::list_customers;

pub(crate) use customers_repository::update_customer;
