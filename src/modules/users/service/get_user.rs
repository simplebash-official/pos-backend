use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::users::User;

use crate::modules::users::repository;

use mongodb::bson::oid::ObjectId;

/// Unrestricted lookup by id — used cross-module by `auth::service::me`
/// (a caller looking up *themselves*, regardless of role) and
/// `auth::service::list_sessions` (resolving a `userId` filter for the
/// audit log, a separate concern from user-management scope). The
/// `users` module's own routes must never call this directly — see
/// `get_user_for_caller` for the scoped equivalent they use instead.
pub(crate) async fn get_user(db: &Db, id: ObjectId) -> AppResult<User> {
    let document = repository::find_user_by_id(db, id)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND))?;

    Ok(document.into_user())
}
