use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A row-level validation or processing error recorded during a batch import.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportRowError {
    /// 1-indexed row number in the uploaded spreadsheet or CSV.
    pub row_number: u64,
    /// The column/field that caused the error, if identifiable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// Human-readable explanation of why the row failed.
    pub message: String,
    /// Raw unparsed value that failed validation, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_value: Option<String>,
}
