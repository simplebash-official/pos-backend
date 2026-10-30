// Pure business types for the imports feature — no I/O, no Mongo/Axum
// types beyond serde/utoipa derives. Mongo document shapes live in
// `modules::imports::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

mod import_batch;
mod import_batch_list_query;
mod import_batch_list_response;
mod import_options;
mod import_row_error;
mod process_import_request;

pub use import_batch::ImportBatch;
pub use import_batch_list_query::ImportBatchListQuery;
pub use import_batch_list_response::ImportBatchListResponse;
pub use import_options::ImportOptions;
pub use import_row_error::ImportRowError;
pub use process_import_request::ProcessImportRequest;
