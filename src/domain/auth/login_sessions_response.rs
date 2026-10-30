use super::login_session::LoginSession;

use serde::Serialize;
use utoipa::ToSchema;

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
