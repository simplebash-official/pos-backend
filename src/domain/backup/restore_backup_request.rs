use super::backup_export_data::BackupExportData;
use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

/// Request payload to restore the system from a previously exported backup.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupRequest {
    /// The backup data to restore.
    pub backup: BackupExportData,
}
