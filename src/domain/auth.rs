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
    pub email: String,
    pub password: String,
}

/// Response for `POST /auth/login`. `expires_in` (seconds) lets the
/// frontend proactively warn before the token expires, since there is no
/// refresh-token flow to silently extend the session.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub token: String,
    pub expires_in: i64,
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
    pub id: String,
    pub key: String,
    pub user_key: String,
    pub name: String,
    pub email: String,
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
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
    pub user_id: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

/// Response for `GET /auth/sessions`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginSessionsResponse {
    pub sessions: Vec<LoginSession>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
}
