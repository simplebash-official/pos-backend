use super::role::Role;
use super::user_preferences::UserPreferences;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A user account as returned to API clients. `id` (the Mongo `ObjectId` as
/// a hex string) is the route/lookup key and the JWT's `sub` claim; `key` is
/// the prefixed id (see `core::id::generate_id`) reserved for any future
/// module that needs to reference a user (e.g. "created by") without
/// holding its `ObjectId`. Never carries the password hash — that field
/// only exists on `modules::users::model::UserDocument`, which this type is
/// converted from, and this type is never constructed from raw user input.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct User {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key identifying this user (e.g. usr_...).
    pub key: String,
    /// Full display name.
    pub name: String,
    /// Unique login username (unique within the shop).
    pub username: String,
    /// User permission role (admin, manager, or staff).
    pub role: Role,
    /// The fixed permission set this role carries, computed from
    /// `core::constants::roles::default_permissions(role)` — never stored,
    /// always derived fresh so it can never drift from the role. This is
    /// what the frontend's `AuthUser.permissions` reads (from `/auth/login`
    /// and `/auth/me`'s `user` object) to gate UI, since permissions
    /// otherwise live only inside the opaque JWT `token`.
    pub permissions: Vec<String>,
    /// Whether user account is active.
    pub is_active: bool,
    /// Whether this user account has a security PIN configured for register overrides.
    #[serde(default)]
    pub has_pin: bool,
    /// This login's own UI preferences (e.g. the billing catalog sort).
    /// Always present in responses; unset values are omitted inside.
    #[serde(default)]
    pub preferences: UserPreferences,
    /// Key of the `Employee` HR/commission profile this login belongs to,
    /// if any. Optional — a login can exist with no linked employee profile
    /// only in the legacy/no-employee-yet case; going forward every
    /// `POST /users` call should supply it. There is deliberately no
    /// reverse `user_key` field on `Employee` — "does this employee have a
    /// login" is always resolved live against this field instead (see
    /// `modules::users::service::find_user_summary_by_employee_key`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub employee_key: Option<String>,
    /// Timestamp when user was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when user was last updated.
    pub updated_at: DateTime<Utc>,
}
