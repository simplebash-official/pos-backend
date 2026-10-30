use crate::domain::users::User;

use serde::Serialize;
use utoipa::ToSchema;

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
    /// Display name of the shop (in multi-tenant deployments).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shop_name: Option<String>,
}
