// Pure domain types for full database backup export and transactional restore.

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

/// Request payload to trigger a system data export.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportBackupRequest {
    /// Whether to include client settings in the exported backup.
    #[serde(default)]
    pub include_settings: bool,
    /// Frontend settings snapshot (shop profile, branding, etc.) if requested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
}

/// Request payload to restore the system from a previously exported backup.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupRequest {
    /// The backup data to restore.
    pub backup: BackupExportData,
}

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
