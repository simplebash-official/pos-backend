//! MongoDB document shapes for the batch imports module. Converts into
//! `domain::imports` before route handlers wrap in `ApiResponse<T>`.

mod imports_model;

pub use imports_model::ImportBatchDocument;
pub use imports_model::ImportRowErrorDocument;
