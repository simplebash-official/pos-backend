// Response shape for a shop's POS user account in the operator console.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::users::Role;

/// A single POS staff account belonging to a shop tenant.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShopUserResponse {
    pub id: String,
    pub key: String,
    pub username: String,
    pub name: String,
    pub role: Role,
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<DateTime<Utc>>,
}
