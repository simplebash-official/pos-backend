use super::role::Role;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /users`. Admin-provisioned only — there is no public
/// self-registration endpoint, so `role` is always caller-supplied rather
/// than defaulted.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserRequest {
    /// Full display name.
    pub name: String,
    /// Login username (unique within the shop).
    pub username: String,
    /// Plaintext password to hash and store.
    pub password: String,
    /// Role to assign to new user (admin, manager, or staff).
    pub role: Role,
    /// Key of the `Employee` profile this login belongs to. Optional, but
    /// every login created through the Employee "Create Login" action
    /// supplies it — see `service::create_user`'s validation.
    #[serde(default)]
    pub employee_key: Option<String>,
}
