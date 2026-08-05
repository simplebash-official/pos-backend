// Business rules for user account CRUD/role management: password
// hashing/verification, email uniqueness, and login-credential checking.
// Delegates all Mongo access to `super::repository`.

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::http::StatusCode;
use mongodb::{
    Database,
    bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId},
};

use crate::{
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::users::{CreateUserRequest, UpdateUserRequest, User, UserListQuery, UsersResponse},
    modules::users::{model::UserDocument, repository},
};

/// A deliberately lightweight structural check (single `@`, non-empty local
/// part, dotted domain) rather than a full RFC 5322 validator — matches the
/// bar `suppliers::service::validate_email` already sets (no `regex` crate
/// dependency exists in this codebase).
fn validate_email(email: &str) -> AppResult<()> {
    let is_valid = email.matches('@').count() == 1
        && !email.contains(' ')
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        });

    if !is_valid {
        return Err(AppError::validation("Email is not a valid email address"));
    }
    Ok(())
}

fn validate_password_strength(password: &str) -> AppResult<()> {
    if password.chars().count() < 8 {
        return Err(AppError::validation(
            "Password must be at least 8 characters",
        ));
    }
    Ok(())
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn hash_password(password: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| AppError::internal(format!("failed to hash password: {err}")))
}

fn verify_password(password: &str, hash: &str) -> AppResult<bool> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|err| AppError::internal(format!("stored password hash is invalid: {err}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// Builds the Mongo filter from query params (free-text search across
/// name/email, plus an exact-role filter) — same construction style as
/// `suppliers::service::list_suppliers`. Not paginated: a shop's staff
/// roster is small enough to return in full.
pub(crate) async fn list_users(db: &Database, query: UserListQuery) -> AppResult<UsersResponse> {
    let mut and_clauses: Vec<Document> = Vec::new();

    if let Some(search) = query.search.filter(|s| !s.is_empty()) {
        let pattern = crate::core::utils::build_bson_regex(&search);
        and_clauses.push(doc! {
            "$or": [
                { "name": { "$regex": pattern.clone() } },
                { "email": { "$regex": pattern } },
            ]
        });
    }
    if let Some(role) = query.role {
        and_clauses.push(doc! { "role": role.as_str() });
    }

    let filter = if and_clauses.is_empty() {
        Document::new()
    } else {
        doc! { "$and": and_clauses }
    };

    let documents = repository::list_users(db, filter).await?;
    let users = documents.into_iter().map(UserDocument::into_user).collect();

    Ok(UsersResponse { users })
}

pub(crate) async fn get_user(db: &Database, id: ObjectId) -> AppResult<User> {
    let document = repository::find_user_by_id(db, id)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND))?;

    Ok(document.into_user())
}

/// Admin-provisioned account creation — there is no public self-registration
/// endpoint. `pub`, not `pub(crate)`, so `src/bin/seed_admin.rs` (a separate
/// binary crate) can create the bootstrap Admin through this exact
/// validation/hashing path instead of duplicating Argon2 logic there.
pub async fn create_user(db: &Database, body: CreateUserRequest) -> AppResult<User> {
    if body.name.trim().is_empty() {
        return Err(AppError::validation("Name is required"));
    }
    validate_email(&body.email)?;
    validate_password_strength(&body.password)?;

    let email = normalize_email(&body.email);
    if repository::find_user_by_email(db, &email).await?.is_some() {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::EMAIL_ALREADY_EXISTS,
            format!("A user with email '{email}' already exists"),
        ));
    }

    let password_hash = hash_password(&body.password)?;
    let now = BsonDateTime::now();
    let document = UserDocument {
        id: None,
        key: generate_id(prefixes::USER),
        name: body.name,
        email,
        password_hash,
        role: body.role,
        is_active: true,
        created_at: now,
        updated_at: now,
    };

    let inserted = repository::insert_user(db, document).await?;
    Ok(inserted.into_user())
}

/// Partial update for `PATCH /users/{id}` — every field in `body` is
/// optional; required fields (name/email) fall back to the existing
/// document's value before re-validation. `password` present means rehash
/// to the new value; absent leaves the stored hash untouched. Email
/// uniqueness is only re-checked when the email actually changed, so
/// re-saving a user's existing email never trips the uniqueness guard
/// against itself.
pub(crate) async fn update_user(
    db: &Database,
    id: ObjectId,
    body: UpdateUserRequest,
) -> AppResult<User> {
    let existing = repository::find_user_by_id(db, id)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND))?;

    let name = body.name.unwrap_or(existing.name);
    if name.trim().is_empty() {
        return Err(AppError::validation("Name is required"));
    }

    let email = match body.email {
        Some(email) => {
            validate_email(&email)?;
            let normalized = normalize_email(&email);
            if normalized != existing.email
                && repository::find_user_by_email(db, &normalized)
                    .await?
                    .is_some()
            {
                return Err(AppError::custom(
                    StatusCode::CONFLICT,
                    codes::EMAIL_ALREADY_EXISTS,
                    format!("A user with email '{normalized}' already exists"),
                ));
            }
            normalized
        }
        None => existing.email,
    };

    let mut set_doc = doc! {
        "name": &name,
        "email": &email,
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

    let updated = repository::update_user(db, id, set_doc)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND))?;

    Ok(updated.into_user())
}

/// Refuses to delete a caller's own account (400 `CANNOT_DELETE_SELF`) —
/// the common accidental-lockout guard. Does not guard against deleting the
/// last remaining Admin: that check is inherently racy under concurrent
/// requests and out of scope here.
pub(crate) async fn delete_user(
    db: &Database,
    id: ObjectId,
    caller_user_id: &str,
) -> AppResult<User> {
    let existing = repository::find_user_by_id(db, id)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND))?;

    if existing
        .id
        .is_some_and(|existing_id| existing_id.to_hex() == caller_user_id)
    {
        return Err(AppError::validation_with_code(
            "You cannot delete your own account",
            codes::CANNOT_DELETE_SELF,
        ));
    }

    let deleted = repository::delete_user(db, id)
        .await?
        .ok_or_else(|| AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND))?;

    Ok(deleted.into_user())
}

/// Cross-module entry point `auth::service::login` calls. "No such email"
/// and "wrong password" both collapse to the same 401
/// `INVALID_CREDENTIALS` (never distinguish which — avoids email
/// enumeration); a correct password against a deactivated account gets its
/// own 401 `USER_INACTIVE`, since at that point the caller has already
/// proven they know the real password.
pub(crate) async fn verify_credentials(
    db: &Database,
    email: &str,
    password: &str,
) -> AppResult<User> {
    let normalized = normalize_email(email);
    let document = repository::find_user_by_email(db, &normalized)
        .await?
        .ok_or_else(|| {
            AppError::unauthorized_with_code(
                "Invalid email or password",
                codes::INVALID_CREDENTIALS,
            )
        })?;

    if !verify_password(password, &document.password_hash)? {
        return Err(AppError::unauthorized_with_code(
            "Invalid email or password",
            codes::INVALID_CREDENTIALS,
        ));
    }

    if !document.is_active {
        return Err(AppError::unauthorized_with_code(
            "This account has been deactivated",
            codes::USER_INACTIVE,
        ));
    }

    Ok(document.into_user())
}
