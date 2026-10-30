use super::create_user::create_user;
use super::manageable_roles::manageable_roles;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::users::{CreateUserRequest, Role, User};

/// Scoped creation for `POST /users` — 403s (not 404, since there's no
/// existing resource to hide) if `body.role` is outside
/// `manageable_roles(caller_role)`, e.g. a Manager trying to create another
/// Manager, or anyone trying to create an Admin.
pub(crate) async fn create_user_for_caller(
    db: &Db,
    body: CreateUserRequest,
    caller_role: Role,
) -> AppResult<User> {
    crate::core::logging::domain::tracked("users.created", async move {
        if !manageable_roles(caller_role).contains(&body.role) {
            return Err(AppError::forbidden_with_code(
                format!(
                    "You are not allowed to create a {} account",
                    body.role.as_str()
                ),
                codes::PERMISSION_DENIED,
            ));
        }
        create_user(db, body).await
    })
    .await
}
