// Representation of a single login session record for operator audit.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::domain::users::Role;

/// Details of a single user login session.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShopLoginItem {
    pub id: String,
    pub key: String,
    pub user_key: String,
    pub name: String,
    pub username: String,
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    pub logged_in_at: DateTime<Utc>,
}
