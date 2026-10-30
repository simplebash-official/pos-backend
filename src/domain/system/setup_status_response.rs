use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Detailed setup status response checked on application boot.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatusResponse {
    /// Whether initial system setup has been completed.
    pub setup_completed: bool,
    /// Whether this is the first execution/installation of the application.
    pub is_first_run: bool,
    /// Unique installation identifier.
    pub installation_id: Option<String>,
    /// UTC timestamp when the application was initially installed/run.
    pub installed_at: Option<String>,
    /// UTC timestamp when initial setup was completed.
    pub setup_completed_at: Option<String>,
    /// Whether sample demo data was loaded during setup.
    pub sample_data_loaded: Option<bool>,
    /// Running application version.
    pub app_version: String,
    /// Operating system platform (e.g., "macos", "windows", "linux").
    pub platform: String,
}
