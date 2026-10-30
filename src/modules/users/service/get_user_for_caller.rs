use super::ensure_manageable::ensure_manageable;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::users::{Role, User};

use crate::modules::users::repository;

use mongodb::bson::oid::ObjectId;

/// Scoped lookup for `GET /users/{id}` — 404s if `id` resolves to an
/// account outside `manageable_roles(caller_role)` (see `ensure_manageable`).
pub(crate) async fn get_user_for_caller(
    db: &Db,
    id: ObjectId,
    caller_role: Role,
) -> AppResult<User> {
    let document = repository::find_user_by_id(db, id)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND))?;
    ensure_manageable(caller_role, document.role)?;

    Ok(document.into_user())
}
