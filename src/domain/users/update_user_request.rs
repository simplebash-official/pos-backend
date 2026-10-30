use super::role::Role;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PATCH /users/{id}`. Every field optional so a client sends
/// only what changed — `service::update_user` fills in omitted fields from
/// the existing document rather than clearing them (same convention as
/// `suppliers::service::update_supplier`). `password` present means
/// "rehash to this new password"; absent means "leave it untouched".
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserRequest {
    /// Updated display name.
    pub name: Option<String>,
    /// Updated login username.
    pub username: Option<String>,
    /// New plaintext password, if changing password.
    pub password: Option<String>,
    /// Updated user role.
    pub role: Option<Role>,
    /// Updated active status flag.
    pub is_active: Option<bool>,
    /// Updated linked employee key.
    pub employee_key: Option<String>,
}
