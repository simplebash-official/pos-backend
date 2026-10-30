//! Pure business types for the imports feature — no I/O, no Mongo/Axum
//! types beyond serde/utoipa derives. Mongo document shapes live in
//! `modules::imports::model` and convert into these before a handler wraps
//! them in `core::response::ApiResponse<T>`.

mod domain_imports;

pub use domain_imports::ImportBatch;
pub use domain_imports::ImportBatchListQuery;
pub use domain_imports::ImportBatchListResponse;
pub use domain_imports::ImportOptions;
pub use domain_imports::ImportRowError;
pub use domain_imports::ProcessImportRequest;
