use super::ensure_manageable::ensure_manageable;
use super::hash_password::hash_password;
use super::manageable_roles::manageable_roles;
use super::normalize_username::normalize_username;
use super::validate_password_strength::validate_password_strength;
use super::validate_username::validate_username;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::users::{Role, UpdateUserRequest, User};
use crate::modules::employees;

use crate::modules::users::repository;

use axum::http::StatusCode;
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{DateTime as BsonDateTime, doc};

/// Partial update for `PATCH /users/{id}` — every field in `body` is
/// optional; required fields (name/username) fall back to the existing
/// document's value before re-validation. `password` present means rehash
/// to the new value; absent leaves the stored hash untouched. Username
/// uniqueness is only re-checked when the username actually changed, so
/// re-saving a user's existing username never trips the uniqueness guard
/// against itself. `caller_role` gates two things: the *existing* account
/// must be in `manageable_roles(caller_role)` (404 otherwise), and if
/// `body.role` requests a role change, the *new* role must be too (403
/// otherwise) — so a Manager can never promote a Staff account to Manager,
/// and no one can ever promote anything to Admin through this endpoint.
pub(crate) async fn update_user(
    db: &Db,
    id: ObjectId,
    body: UpdateUserRequest,
    caller_role: Role,
) -> AppResult<User> {
    crate::core::logging::domain::tracked("users.updated", async move {
        let existing = repository::find_user_by_id(db, id).await?.ok_or_else(|| {
            AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND)
        })?;
        ensure_manageable(caller_role, existing.role)?;

        if let Some(new_role) = body.role
            && !manageable_roles(caller_role).contains(&new_role)
        {
            return Err(AppError::forbidden_with_code(
                format!("You are not allowed to set role to {}", new_role.as_str()),
                codes::PERMISSION_DENIED,
            ));
        }

        let name = body.name.unwrap_or(existing.name);
        if name.trim().is_empty() {
            return Err(AppError::validation("Name is required"));
        }

        let username = match body.username {
            Some(username) => {
                let normalized = normalize_username(&username);
                validate_username(&normalized)?;
                if normalized != existing.username
                    && repository::find_user_by_username(db, &normalized)
                        .await?
                        .is_some()
                {
                    return Err(AppError::custom(
                        StatusCode::CONFLICT,
                        codes::USERNAME_ALREADY_EXISTS,
                        format!("A user with username '{normalized}' already exists"),
                    ));
                }
                normalized
            }
            None => existing.username,
        };

        let mut set_doc = doc! {
            "name": &name,
            "username": &username,
            "updated_at": BsonDateTime::now(),
        };
        if let Some(password) = body.password {
            validate_password_strength(&password)?;
            set_doc.insert("password_hash", hash_password(&password)?);
        }
        if let Some(role) = body.role {
            set_doc.insert("role", role.as_str());
        }
        if let Some(is_active) = body.is_active {
            set_doc.insert("is_active", is_active);
        }
        let old_employee_key = existing.employee_key.clone();
        let mut new_employee_key: Option<String> = None;
        if let Some(employee_key) = body.employee_key {
            if Some(&employee_key) != existing.employee_key.as_ref() {
                employees::service::get_employee_by_key(db, &employee_key).await?;
                if repository::find_user_by_employee_key(db, &employee_key)
                    .await?
                    .is_some()
                {
                    return Err(AppError::conflict(
                        codes::EMPLOYEE_ALREADY_HAS_LOGIN,
                        "This employee already has a login account",
                    ));
                }
            }
            new_employee_key = Some(employee_key.clone());
            set_doc.insert("employee_key", employee_key);
        }

        let updated = repository::update_user(db, id, set_doc)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND)
            })?;

        // Bump both the old and new linked employee's `updated_at`/`version` so
        // the next sync delta re-delivers each with its `login` field current —
        // see `employees::service::touch_by_key`'s doc comment.
        if new_employee_key.is_some() && new_employee_key != old_employee_key {
            if let Some(key) = &new_employee_key {
                employees::service::touch_by_key(db, key).await?;
            }
            if let Some(key) = &old_employee_key {
                employees::service::touch_by_key(db, key).await?;
            }
        }

        let user = updated.into_user();
        // A deactivation or role change must reach the user's live tokens now.
        crate::core::middleware::auth::invalidate_account_status(&user.id);
        Ok(user)
    })
    .await
}
