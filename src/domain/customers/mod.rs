//! Pure business types for the customers feature — no I/O, no Mongo/Axum
//! types beyond serde/utoipa derives. Mongo document shapes live in
//! `modules::customers::model` and convert into these before a handler wraps
//! them in `core::response::ApiResponse<T>`.

mod domain_customers;

pub use domain_customers::CreateCustomerRequest;
pub use domain_customers::Customer;
pub use domain_customers::CustomerListQuery;
pub use domain_customers::CustomerListResponse;
pub use domain_customers::CustomerStats;
pub use domain_customers::CustomerTagsResponse;
pub use domain_customers::DeleteCustomersRequest;
pub use domain_customers::DeleteCustomersResponse;
pub use domain_customers::UpdateCustomerRequest;
