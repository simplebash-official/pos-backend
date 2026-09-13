// Pure business types for user accounts and role management — no I/O, no
// Mongo/Axum types beyond serde/utoipa derives. Mongo document shape lives
// in `modules::users::model` (private) and converts into these before a
// handler wraps them in `core::response::ApiResponse<T>`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// The three fixed roles this system supports. Each maps to a hardcoded
/// default permission set (`core::constants::roles::default_permissions`) —
/// there are no custom/DB-defined roles. Serializes to a lowercase string
/// (`"admin"`/`"manager"`/`"staff"`) so it drops straight into a JWT claim
/// and matches the string `core::middleware::auth::AdminUser` already
/// compared against before this type existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Manager,
    Staff,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Manager => "manager",
            Role::Staff => "staff",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "admin" => Some(Role::Admin),
            "manager" => Some(Role::Manager),
            "staff" => Some(Role::Staff),
            _ => None,
        }
    }
}

impl std::str::FromStr for Role {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Role::from_str(s).ok_or(())
    }
}

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
    /// Unique email address.
    pub email: String,
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

/// Body for `POST /users`. Admin-provisioned only — there is no public
/// self-registration endpoint, so `role` is always caller-supplied rather
/// than defaulted.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserRequest {
    /// Full display name.
    pub name: String,
    /// Login email address.
    pub email: String,
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
    /// Updated login email address.
    pub email: Option<String>,
    /// New plaintext password, if changing password.
    pub password: Option<String>,
    /// Updated user role.
    pub role: Option<Role>,
    /// Updated active status flag.
    pub is_active: Option<bool>,
    /// Updated linked employee key.
    pub employee_key: Option<String>,
}

/// Query params for `GET /users`. `search` matches against name/email.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct UserListQuery {
    /// Search term matching name or email.
    pub search: Option<String>,
    /// Filter users by role.
    pub role: Option<Role>,
}

/// Response for `GET /users`. Not paginated — a shop's staff roster is
/// small enough to return in full, matching `suppliers`' list contract.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsersResponse {
    /// Complete list of manageable users.
    pub users: Vec<User>,
}
