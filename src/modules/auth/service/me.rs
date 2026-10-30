use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};
use crate::domain::users::User;
use crate::modules::users::service as users_service;
use mongodb::bson::oid::ObjectId;

/// Re-reads the account from Mongo rather than only decoding the JWT — the
/// one place a caller can discover mid-session that their account was
/// deactivated (or deleted) *since* their token was issued, since there is
/// no revocation/blacklist infra to push that information any other way.
pub(crate) async fn me(db: &Db, user_id: &str, identity_name: Option<&str>) -> AppResult<User> {
    let gone =
        || AppError::unauthorized_with_code("Account no longer exists", codes::USER_NOT_FOUND);
    let user = match ObjectId::parse_str(user_id) {
        Ok(object_id) => users_service::get_user(db, object_id)
            .await
            .map_err(|err| match err {
                AppError::NotFound { .. } => gone(),
                other => other,
            })?,
        // An identity-server token's subject is an `acc_...` id, not a local
        // ObjectId: an owner-scope identity token maps to the shop's Admin.
        Err(_) => users_service::find_user_for_identity(db, identity_name)
            .await?
            .ok_or_else(gone)?,
    };

    if !user.is_active {
        return Err(AppError::unauthorized_with_code(
            "This account has been deactivated",
            codes::USER_INACTIVE,
        ));
    }

    Ok(user)
}
