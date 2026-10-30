// Business rules for customer CRUD/listing: required-field validation,
// quick-create support, phone/email format checks, search/filtering,
// and the financial debt guard that prevents deleting a customer with
// an outstanding balance. Delegates all Mongo access to `super::repository`.

mod apply_financial_delta;
mod create_customer;
mod delete_customer;
mod delete_customers;
mod get_customer;
mod get_customer_by_key;
mod get_customer_stats;
mod get_customer_tags;
mod hydrate_sync_documents;
mod list_customers;
mod replace_customer;
mod sanitize_string;
mod sanitize_tags;
mod update_customer;
mod validate_email;
mod validate_phone_number;

pub(crate) use apply_financial_delta::apply_financial_delta;
pub use create_customer::create_customer;
pub use delete_customer::delete_customer;
pub use delete_customers::delete_customers;
pub use get_customer::get_customer;
pub(crate) use get_customer_by_key::get_customer_by_key;
pub use get_customer_stats::get_customer_stats;
pub use get_customer_tags::get_customer_tags;
pub(crate) use hydrate_sync_documents::hydrate_sync_documents;
pub use list_customers::list_customers;
pub use replace_customer::replace_customer;
pub use update_customer::update_customer;
