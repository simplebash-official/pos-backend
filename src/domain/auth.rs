// Pure business types for the auth feature — login, "who am I", and the
// login-session audit log. No I/O, no Mongo/Axum types beyond serde/utoipa
// derives. `auth` never defines its own user-shaped type: it reuses
// `domain::users::User` so the frontend has exactly one user shape to model.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::domain::users::{Role, User};

/// Body for `POST /auth/login`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    /// User's login email address.
    pub email: String,
    /// User's plain text password.
    pub password: String,
    /// Shop (tenant) the account belongs to. Required on multi-tenant
    /// deployments (`TENANT_MODE=multi`), ignored on single-shop ones.
    #[serde(default, alias = "shop_code")]
    pub shop_code: Option<String>,
}

/// Response for `POST /auth/login`. `expires_in` (seconds) lets the
/// frontend proactively warn before the token expires, since there is no
/// refresh-token flow to silently extend the session.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    /// Signed JWT access token for authenticating subsequent requests.
    pub token: String,
    /// Number of seconds until the access token expires.
    pub expires_in: i64,
    /// Profile details of the logged-in user.
    pub user: User,
}

/// A single login event, as returned by `GET /auth/sessions`. `name`/
/// `email`/`role` are a snapshot of the account *at the moment of login*,
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
    /// User's email at the time of login.
    pub email: String,
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

/// Query params for `GET /auth/sessions`. `user_id` filters to one
/// account's login history; omitted returns every account's history
/// (subject to `SESSIONS_VIEW` gating). Paginated (unlike `suppliers`'
/// unpaginated lists) since this is an append-only log that grows without
/// bound, unlike a shop's small, roughly-static staff/supplier roster.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct LoginSessionListQuery {
    /// Optional user ID or key to filter sessions for a specific user.
    pub user_id: Option<String>,
    /// Page number for pagination (defaults to 1).
    pub page: Option<u64>,
    /// Number of session records to return per page.
    pub limit: Option<u64>,
}

/// Response for `GET /auth/sessions`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginSessionsResponse {
    /// List of login sessions for the current page.
    pub sessions: Vec<LoginSession>,
    /// Total count of matching login sessions across all pages.
    pub total: u64,
    /// Current page number.
    pub page: u64,
    /// Maximum number of items returned per page.
    pub limit: u64,
}
