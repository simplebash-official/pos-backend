use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
