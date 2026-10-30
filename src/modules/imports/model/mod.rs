// MongoDB document shapes for the batch imports module. Converts into
// `domain::imports` before route handlers wrap in `ApiResponse<T>`.

mod import_batch_document;
mod import_row_error_document;

pub use import_batch_document::ImportBatchDocument;
pub use import_row_error_document::ImportRowErrorDocument;
