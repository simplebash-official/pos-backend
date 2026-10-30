use serde::{Deserialize, Serialize};

use utoipa::ToSchema;

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
