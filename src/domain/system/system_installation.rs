use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// System installation metadata entity.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SystemInstallation {
    pub key: String,
    pub id: String,
    pub installation_id: String,
    pub app_version: String,
    pub platform: String,
    pub installed_at: String,
    pub setup_completed: bool,
    pub setup_completed_at: Option<String>,
    pub sample_data_loaded: bool,
    pub created_at: String,
    pub updated_at: String,
}
