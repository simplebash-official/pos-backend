use crate::domain::users::Role;
use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// A single login event, as returned by `GET /auth/sessions`. `name`/
/// `username`/`role` are a snapshot of the account *at the moment of login*,
/// not a live join against the current account — see
/// `modules::auth::model::LoginSessionDocument` for why. `ip_address`/
/// `user_agent` are best-effort (absent when no proxy header was set).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginSession {
    /// Unique database ID of the login session.
    pub id: String,
    /// Unique business key of the login session.
    pub key: String,
    /// Unique key of the user who logged in.
    pub user_key: String,
    /// User's name at the time of login.
    pub name: String,
    /// User's username at the time of login.
    pub username: String,
    /// User's role at the time of login.
    pub role: Role,
    /// Originating IP address of the login request, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    /// Client browser or app info used for logging in, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// Date and time when the user logged in.
    pub logged_in_at: DateTime<Utc>,
}
