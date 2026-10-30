// Pure business types for the customers feature — no I/O, no Mongo/Axum
// types beyond serde/utoipa derives. Mongo document shapes live in
// `modules::customers::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

mod create_customer_request;
mod customer;
mod customer_list_query;
mod customer_list_response;
mod customer_stats;
mod customer_tags_response;
mod delete_customers_request;
mod delete_customers_response;
mod update_customer_request;

pub use create_customer_request::CreateCustomerRequest;
pub use customer::Customer;
pub use customer_list_query::CustomerListQuery;
pub use customer_list_response::CustomerListResponse;
pub use customer_stats::CustomerStats;
pub use customer_tags_response::CustomerTagsResponse;
pub use delete_customers_request::DeleteCustomersRequest;
pub use delete_customers_response::DeleteCustomersResponse;
pub use update_customer_request::UpdateCustomerRequest;
