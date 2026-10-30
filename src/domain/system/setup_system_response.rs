use crate::domain::users::User;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
