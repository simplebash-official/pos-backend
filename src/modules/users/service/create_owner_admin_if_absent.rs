use super::find_admin_document::find_admin_document;
use super::normalize_username::normalize_username;

use super::validate_owner_login::validate_owner_login;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;

use crate::domain::users::Role;

use crate::modules::users::model::UserDocument;
use crate::modules::users::repository;

use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Creates the owner's `admin` login for a freshly provisioned shop from a
/// password hash the identity service already made (from the admin password the
/// owner chose at shop creation), so the plaintext never reaches this service.
/// Returns `false` when the shop already has an Admin: that Admin's display name
/// is refreshed with identity's, its hash is never touched, so replaying a
/// provisioning request is harmless. `pub` for the same reason as `create_user`:
/// the tenants module reaches it across the private `repository` boundary.
pub async fn create_owner_admin_if_absent(
    db: &Db,
    name: &str,
    username: &str,
    password_hash: &str,
) -> AppResult<bool> {
    validate_owner_login(name, username, password_hash)?;

    // Replays are how identity re-syncs a profile: when the Admin already
    // exists, identity is the source of truth for its display name. The password
    // hash is deliberately left alone - the shop may have changed it locally.
    if let Some(existing) = find_admin_document(db).await? {
        let name = name.trim();
        if existing.name != name
            && let Some(id) = existing.id
        {
            repository::update_user(db, id, doc! { "name": name }).await?;
        }
        return Ok(false);
    }

    let username = normalize_username(username);
    if repository::find_user_by_username(db, &username)
        .await?
        .is_some()
    {
        return Err(AppError::conflict(
            codes::USERNAME_ALREADY_EXISTS,
            format!("A user with username '{username}' already exists"),
        ));
    }

    let now = BsonDateTime::now();
    let document = UserDocument {
        id: None,
        key: generate_id(prefixes::USER),
        name: name.trim().to_string(),
        username,
        password_hash: password_hash.to_string(),
        role: Role::Admin,
        is_active: true,
        employee_key: None,
        preferences: None,
        pin_hash: None,
        created_at: now,
        updated_at: now,
    };
    repository::insert_user(db, document).await?;
    Ok(true)
}
