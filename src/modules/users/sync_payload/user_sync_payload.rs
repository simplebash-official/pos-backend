use chrono::{DateTime, Utc};

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserSyncPayload {
    /// Legacy hex id: preserved on every replica because login tokens carry it.
    pub id: String,
    pub key: String,
    pub name: String,
    pub username: String,
    pub password_hash: String,
    pub role: String,
    pub is_active: bool,
    pub employee_key: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub version: i64,
    pub deleted_at: Option<DateTime<Utc>>,
}
