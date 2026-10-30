use super::me::me;
use crate::clients::db::Db;
use crate::core::error::{AppError, AppResult};
use crate::domain::users::{UpdateMyPreferencesRequest, User};
use crate::modules::users::service as users_service;
use mongodb::bson::oid::ObjectId;

/// Backs `PATCH /auth/me/preferences`: resolves the caller exactly like `me`
/// (so identity-server tokens work too), then updates only their own record.
pub(crate) async fn update_my_preferences(
    db: &Db,
    user_id: &str,
    identity_name: Option<&str>,
    changes: UpdateMyPreferencesRequest,
) -> AppResult<User> {
    let current = me(db, user_id, identity_name).await?;
    let object_id = ObjectId::parse_str(&current.id)
        .map_err(|_| AppError::unauthorized("Invalid token subject"))?;
    users_service::update_own_preferences(db, object_id, changes).await
}
