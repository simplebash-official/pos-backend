//! Pure business types for the suppliers feature — no I/O, no Mongo/Axum
//! types beyond serde/utoipa derives. Mongo document shapes live in
//! `modules::suppliers::model` and convert into these before a handler wraps
//! them in `core::response::ApiResponse<T>`.

mod domain_suppliers;

pub use domain_suppliers::ContactPerson;
pub use domain_suppliers::CreateSupplierRequest;
pub use domain_suppliers::DeleteSuppliersRequest;
pub use domain_suppliers::DeleteSuppliersResponse;
pub use domain_suppliers::Supplier;
pub use domain_suppliers::SupplierCategoriesResponse;
pub use domain_suppliers::SupplierListQuery;
pub use domain_suppliers::SupplierStats;
pub use domain_suppliers::SuppliersResponse;
pub use domain_suppliers::UpdateSupplierRequest;
