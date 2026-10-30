use super::ensure_manageable::ensure_manageable;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::users::{Role, User};
use crate::modules::employees;

use crate::modules::users::repository;

use mongodb::bson::oid::ObjectId;

/// 404s if `id` resolves to an account outside `manageable_roles(caller_role)`
/// (see `ensure_manageable`) — note this also means a caller can never
/// delete their *own* account through this endpoint, since no role is ever
/// in its own `manageable_roles` set (Admin doesn't manage Admin, Manager
/// doesn't manage Manager); a dedicated self-delete guard is therefore
/// unnecessary and was removed rather than kept as unreachable code.
pub(crate) async fn delete_user(db: &Db, id: ObjectId, caller_role: Role) -> AppResult<User> {
    crate::core::logging::domain::tracked("users.deleted", async move {
        let existing = repository::find_user_by_id(db, id).await?.ok_or_else(|| {
            AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND)
        })?;
        ensure_manageable(caller_role, existing.role)?;

        let deleted = repository::delete_user(db, id).await?.ok_or_else(|| {
            AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND)
        })?;

        if let Some(employee_key) = &deleted.employee_key {
            // The employee just lost its login — see
            // `employees::service::touch_by_key`'s doc comment.
            employees::service::touch_by_key(db, employee_key).await?;
        }

        let user = deleted.into_user();
        crate::core::middleware::auth::invalidate_account_status(&user.id);
        Ok(user)
    })
    .await
}
