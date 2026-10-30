use super::role::Role;

use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /users`. `search` matches against name/username.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct UserListQuery {
    /// Search term matching name or username.
    pub search: Option<String>,
    /// Filter users by role.
    pub role: Option<Role>,
}
