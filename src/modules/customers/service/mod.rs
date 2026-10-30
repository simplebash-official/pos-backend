//! Business rules for customer CRUD/listing: required-field validation,
//! quick-create support, phone/email format checks, search/filtering,
//! and the financial debt guard that prevents deleting a customer with
//! an outstanding balance. Delegates all Mongo access to `super::repository`.

mod customers_service;

pub(crate) use customers_service::apply_financial_delta;
pub use customers_service::create_customer;
pub use customers_service::delete_customer;
pub use customers_service::delete_customers;
pub use customers_service::get_customer;
pub(crate) use customers_service::get_customer_by_key;
pub use customers_service::get_customer_stats;
pub use customers_service::get_customer_tags;
pub(crate) use customers_service::hydrate_sync_documents;
pub use customers_service::list_customers;
pub use customers_service::replace_customer;
pub use customers_service::update_customer;
