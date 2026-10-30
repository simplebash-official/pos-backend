use super::import_row_error::ImportRowError;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
