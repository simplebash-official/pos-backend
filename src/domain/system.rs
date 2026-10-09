// Pure domain types for initial system installation, onboarding status, and database bootstrap.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::users::User;

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

/// Request payload to perform initial system setup and database initialization.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupSystemRequest {
    /// If true, populates sample categories, inventory products, customers, and suppliers.
    /// If false, leaves all catalog and transactional tables clean and empty.
    #[serde(alias = "load_sample_data")]
    pub load_sample_data: bool,
    /// Administrator display name (defaults to "System Admin").
    #[serde(default, alias = "admin_name")]
    pub admin_name: Option<String>,
    /// Administrator password. Required; the Admin username is always `admin`.
    #[serde(default, alias = "admin_password")]
    pub admin_password: Option<String>,
}

/// Result returned after system setup and database initialization.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetupSystemResponse {
    /// Confirms setup is completed.
    pub setup_completed: bool,
    /// Whether sample demo data was loaded.
    pub sample_data_loaded: bool,
    /// The administrator username configured (always `admin`).
    pub admin_username: String,
    /// Authentication JWT token for the configured admin account to enable auto-login.
    pub token: Option<String>,
    /// Profile of the authenticated administrator user.
    pub user: Option<User>,
    /// Human-readable summary message.
    pub message: String,
}

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
