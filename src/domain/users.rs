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
}

/// A user account as returned to API clients. `id` (the Mongo `ObjectId` as
/// a hex string) is the route/lookup key and the JWT's `sub` claim; `key` is
/// the prefixed id (see `core::id::generate_id`) reserved for any future
/// module that needs to reference a user (e.g. "created by") without
/// holding its `ObjectId`. Never carries the password hash — that field
/// only exists on `modules::users::model::UserDocument`, which this type is
/// converted from, and this type is never constructed from raw user input.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub key: String,
    pub name: String,
    pub email: String,
    pub role: Role,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Body for `POST /users`. Admin-provisioned only — there is no public
/// self-registration endpoint, so `role` is always caller-supplied rather
/// than defaulted.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
    pub password: String,
    pub role: Role,
}

/// Body for `PATCH /users/{id}`. Every field optional so a client sends
/// only what changed — `service::update_user` fills in omitted fields from
/// the existing document rather than clearing them (same convention as
/// `suppliers::service::update_supplier`). `password` present means
/// "rehash to this new password"; absent means "leave it untouched".
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserRequest {
    pub name: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub role: Option<Role>,
    pub is_active: Option<bool>,
}

/// Query params for `GET /users`. `search` matches against name/email.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct UserListQuery {
    pub search: Option<String>,
    pub role: Option<Role>,
}

/// Response for `GET /users`. Not paginated — a shop's staff roster is
/// small enough to return in full, matching `suppliers`' list contract.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsersResponse {
    pub users: Vec<User>,
}
