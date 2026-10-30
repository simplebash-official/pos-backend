// Pure business types for the suppliers feature — no I/O, no Mongo/Axum
// types beyond serde/utoipa derives. Mongo document shapes live in
// `modules::suppliers::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

mod contact_person;
mod create_supplier_request;
mod delete_suppliers_request;
mod delete_suppliers_response;
mod supplier;
mod supplier_categories_response;
mod supplier_list_query;
mod supplier_stats;
mod suppliers_response;
mod update_supplier_request;

pub use contact_person::ContactPerson;
pub use create_supplier_request::CreateSupplierRequest;
pub use delete_suppliers_request::DeleteSuppliersRequest;
pub use delete_suppliers_response::DeleteSuppliersResponse;
pub use supplier::Supplier;
pub use supplier_categories_response::SupplierCategoriesResponse;
pub use supplier_list_query::SupplierListQuery;
pub use supplier_stats::SupplierStats;
pub use suppliers_response::SuppliersResponse;
pub use update_supplier_request::UpdateSupplierRequest;
