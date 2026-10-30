use super::manageable_roles::manageable_roles;

use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::users::Role;

/// 404s (rather than 403s) when `target_role` is outside what `caller_role`
/// may manage — the target simply doesn't exist from this caller's point of
/// view, so its existence isn't confirmed/denied any differently than a
/// truly-missing id would be (e.g. a Manager gets the same response
/// whether a given id belongs to another Manager or to no one at all).
pub(super) fn ensure_manageable(caller_role: Role, target_role: Role) -> AppResult<()> {
    if manageable_roles(caller_role).contains(&target_role) {
        Ok(())
    } else {
        Err(AppError::not_found_with_code(
            "User not found",
            codes::USER_NOT_FOUND,
        ))
    }
}
