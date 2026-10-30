use super::current_user::CurrentUser;
use crate::app::AppState;

use crate::core::error::AppError;

use crate::domain::users::Role;
use axum::extract::FromRequestParts;

use axum::http::request::Parts;

/// Identifies a caller whose JWT carries `role: Role::Admin`. This is the
/// pattern to reach for any "must literally be Admin" endpoint (not just
/// inventory's category management) — add `AdminUser` as a handler
/// argument the same way `CurrentUser` gates a route to "any authenticated
/// user", and it rejects with 403 before the handler body runs. For
/// finer-grained gating (e.g. "any role with a specific permission"),
/// prefer `CurrentUser::require_permission` instead.
#[derive(Debug, Clone)]
pub struct AdminUser(pub CurrentUser);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let current_user = CurrentUser::from_request_parts(parts, state).await?;

        if current_user.role != Some(Role::Admin) {
            return Err(AppError::forbidden_with_code(
                "Admin access required",
                crate::core::constants::codes::ADMIN_REQUIRED,
            ));
        }

        Ok(AdminUser(current_user))
    }
}
