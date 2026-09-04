// Pure business types for the imports feature — no I/O, no Mongo/Axum
// types beyond serde/utoipa derives. Mongo document shapes live in
// `modules::imports::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::domain::inventory::PaginationMeta;

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

/// Optional processing switches submitted with a batch import.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    /// If true, rows without a barcode will have a valid EAN-13 barcode auto-generated.
    #[serde(default)]
    pub auto_generate_barcodes: bool,
    /// If true and a referenced category/subcategory does not exist, it is created automatically.
    #[serde(default)]
    pub auto_create_categories: bool,
}

/// Payload sent by the client to trigger server-side batch import processing.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessImportRequest {
    /// Target module identifier (e.g. "inventory", "customers").
    pub target: String,
    /// Original file name uploaded by the user.
    pub file_name: String,
    /// Detected file type ("xlsx", "xls", or "csv").
    pub file_type: String,
    /// File size in bytes.
    #[serde(default)]
    pub file_size_bytes: i64,
    /// Parsed records extracted by the frontend.
    pub rows: Vec<serde_json::Value>,
    /// Optional processing switches.
    #[serde(default)]
    pub options: Option<ImportOptions>,
}

/// A recorded batch import audit record returned to API clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatch {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Unique business key identifying this import batch (e.g. imp_...).
    pub key: String,
    /// Target module identifier (e.g. "inventory").
    pub target: String,
    /// Original file name.
    pub file_name: String,
    /// File format type.
    pub file_type: String,
    /// File size in bytes.
    pub file_size_bytes: i64,
    /// Total count of rows submitted for processing.
    pub total_rows: u64,
    /// Count of records successfully created in the database.
    pub successful_rows: u64,
    /// Count of records that failed validation or insertion.
    pub failed_rows: u64,
    /// Status ("completed", "partially_completed", "failed").
    pub status: String,
    /// Detailed errors for rows that failed.
    pub errors: Vec<ImportRowError>,
    /// Key of the user who initiated the import.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_key: Option<String>,
    /// Name of the user who initiated the import.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by_user_name: Option<String>,
    /// Timestamp when import was recorded.
    pub created_at: DateTime<Utc>,
    /// Timestamp when import finished processing.
    pub updated_at: DateTime<Utc>,
}

/// Query parameters for listing past import batches.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ImportBatchListQuery {
    /// Filter batches by target module (e.g. "inventory").
    pub target: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page limit.
    pub limit: Option<u64>,
}

/// Paginated response for `GET /api/imports`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchListResponse {
    /// List of import batches on the current page.
    pub items: Vec<ImportBatch>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}
