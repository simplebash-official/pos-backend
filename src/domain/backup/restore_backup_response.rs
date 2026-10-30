use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Summary report returned after a successful restore operation.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupResponse {
    /// UTC ISO-8601 timestamp when the restore finished.
    pub restored_at: String,
    /// Number of tables successfully restored.
    pub total_tables: usize,
    /// Total number of records inserted into the database.
    pub total_records: usize,
    /// Breakdown of record counts restored per table name.
    pub restored_counts: HashMap<String, usize>,
    /// Restored frontend settings, if any were present in the backup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
}
