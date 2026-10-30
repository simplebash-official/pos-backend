use super::find_admin_document::find_admin_document;
use super::hash_password::hash_password;

use super::validate_password_strength::validate_password_strength;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::users::User;

use crate::modules::users::repository;

use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Rotates the single Admin account's password in place (its username is always
/// `admin`). `pub`, not `pub(crate)`, so `src/bin/reset_admin.rs` can reach it
/// directly — there is no HTTP path that can do this, since `ensure_manageable`
/// always 404s an Admin target for every caller (see `manageable_roles`) and
/// this deployment has no self-service "forgot password" flow. Requires an
/// Admin to already exist (use `create_user`/`seed_admin` to create the first
/// one); overwrites the existing Admin's password with no undo.
pub async fn reset_admin_credentials(db: &Db, password: &str) -> AppResult<User> {
    validate_password_strength(password)?;

    let admin = find_admin_document(db).await?.ok_or_else(|| {
        AppError::not_found_with_code(
            "No Admin account exists yet; run seed_admin first",
            codes::USER_NOT_FOUND,
        )
    })?;
    let admin_id = admin.id.expect("persisted user document must have an id");

    let set_doc = doc! {
        "password_hash": hash_password(password)?,
        "updated_at": BsonDateTime::now(),
    };

    let updated = repository::update_user(db, admin_id, set_doc)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND))?;

    let user = updated.into_user();
    // Existing tokens for this account must be re-checked on their next use.
    crate::core::middleware::auth::invalidate_account_status(&user.id);
    Ok(user)
}
