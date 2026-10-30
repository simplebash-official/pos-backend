use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::CurrentUser;

use crate::domain::users::Role;

/// Every handler needs the caller's role to pass into `service::*`'s
/// hierarchy checks — `require_any_permission` above it already guarantees
/// `role` is `Some` in practice (only `Role::Admin`/`Role::Manager` are ever
/// granted `USERS_MANAGE`/`USERS_MANAGE_STAFF`), but the claim is still
/// typed `Option<Role>`, so this turns a missing role into a 403 rather
/// than a panic.
pub(super) fn require_caller_role(user: &CurrentUser) -> AppResult<Role> {
    user.role
        .ok_or_else(|| AppError::forbidden("Token is missing a role claim"))
}
