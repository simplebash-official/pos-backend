use super::hash_password::hash_password;
use super::normalize_username::normalize_username;
use super::validate_password_strength::validate_password_strength;
use super::validate_username::validate_username;
use crate::clients::db::Db;
use crate::core::constants::{codes, prefixes, roles};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;

use crate::domain::users::{CreateUserRequest, Role, User};
use crate::modules::employees;
use crate::modules::users::model::UserDocument;
use crate::modules::users::repository;

use axum::http::StatusCode;

use mongodb::bson::DateTime as BsonDateTime;

/// Unrestricted account creation. `pub`, not `pub(crate)`, so
/// `src/bin/seed_admin.rs` (a separate binary crate) can create the
/// bootstrap Admin through this exact validation/hashing path instead of
/// duplicating Argon2 logic there — this is also the *only* path an Admin
/// account can ever be created through, since `create_user_for_caller`
/// (what the HTTP route actually calls) never allows `role: Admin` for any
/// caller (see `manageable_roles`). Enforces the single-Admin invariant
/// this deployment assumes: creating a second Admin is rejected outright,
/// including on a `seed_admin` re-run with a different username.
pub async fn create_user(db: &Db, body: CreateUserRequest) -> AppResult<User> {
    if body.name.trim().is_empty() {
        return Err(AppError::validation("Name is required"));
    }
    let username = normalize_username(&body.username);
    validate_username(&username)?;
    validate_password_strength(&body.password)?;
    if body.role == Role::Admin && username != roles::ADMIN_USERNAME {
        return Err(AppError::validation(format!(
            "The Admin username is always '{}'",
            roles::ADMIN_USERNAME
        )));
    }

    if body.role == Role::Admin && repository::count_users_by_role(db, Role::Admin).await? > 0 {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::ADMIN_ALREADY_EXISTS,
            "An Admin account already exists; this deployment supports only one Admin",
        ));
    }

    if repository::find_user_by_username(db, &username)
        .await?
        .is_some()
    {
        return Err(AppError::conflict(
            codes::USERNAME_ALREADY_EXISTS,
            format!("A user with username '{username}' already exists"),
        ));
    }

    let employee_key = match body.employee_key.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(employee_key) => {
            employees::service::get_employee_by_key(db, employee_key).await?;

            if repository::find_user_by_employee_key(db, employee_key)
                .await?
                .is_some()
            {
                return Err(AppError::conflict(
                    codes::EMPLOYEE_ALREADY_HAS_LOGIN,
                    "This employee already has a login account",
                ));
            }

            Some(employee_key.to_string())
        }
    };

    let password_hash = hash_password(&body.password)?;
    let now = BsonDateTime::now();
    let document = UserDocument {
        id: None,
        key: generate_id(prefixes::USER),
        name: body.name.trim().to_string(),
        username,
        password_hash,
        role: body.role,
        is_active: true,
        employee_key: employee_key.clone(),
        preferences: None,
        pin_hash: None,
        created_at: now,
        updated_at: now,
    };

    let inserted = repository::insert_user(db, document).await?;

    if let Some(employee_key) = &employee_key {
        employees::service::touch_by_key(db, employee_key).await?;
    }

    Ok(inserted.into_user())
}
