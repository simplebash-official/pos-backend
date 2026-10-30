use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Complete structured backup package for all application data.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BackupExportData {
    /// Backup file format version (currently 1).
    pub version: u32,
    /// UTC ISO-8601 timestamp when the export was created.
    pub exported_at: String,
    /// POS application version that generated the backup.
    pub app_version: String,
    /// Operating environment (e.g., "desktop").
    pub environment: String,
    /// Total number of database tables included in the backup.
    pub total_tables: usize,
    /// Total number of individual records across all tables.
    pub total_records: usize,
    /// Map of table names to row records serialized as JSON objects.
    pub tables: HashMap<String, Vec<serde_json::Value>>,
    /// Optional snapshot of frontend settings (shop profile, branding, printer, templates).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
}
