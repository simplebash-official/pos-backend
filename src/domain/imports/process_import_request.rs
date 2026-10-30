use super::import_options::ImportOptions;

use serde::Deserialize;
use utoipa::ToSchema;

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
