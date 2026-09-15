// Business rules for user account CRUD/role management: password
// hashing/verification, email uniqueness, login-credential checking, and
// the Admin/Manager management hierarchy (see `manageable_roles` below).
// Delegates all Mongo access to `super::repository`.

use std::collections::HashMap;

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::http::StatusCode;
use mongodb::bson::{DateTime as BsonDateTime, Document, doc, oid::ObjectId};

use crate::{
    clients::db::Db,
    core::{
        constants::{codes, prefixes},
        error::{AppError, AppResult},
        id::generate_id,
    },
    domain::{
        employees::EmployeeLoginSummary,
        users::{CreateUserRequest, Role, UpdateUserRequest, User, UserListQuery, UsersResponse},
    },
    modules::{
        employees,
        users::{model::UserDocument, repository},
    },
};

/// The roles a caller of `caller_role` may create/view/edit/delete through
/// this module's endpoints: Admin manages Manager and Staff; Manager
/// manages Staff only. An Admin account is never manageable through this
/// API by anyone — not even by another Admin — since this is a
/// single-shop POS deployment that only ever has one Admin, bootstrapped
/// once via `src/bin/seed_admin.rs` (see `create_user`'s single-Admin
/// guard below). Staff never reaches these functions at all (no
/// `users:manage`/`users:manage:staff` permission to get past the route
/// gate in the first place).
fn manageable_roles(caller_role: Role) -> &'static [Role] {
    match caller_role {
        Role::Admin => &[Role::Manager, Role::Staff],
        Role::Manager => &[Role::Staff],
        Role::Staff => &[],
    }
}

/// 404s (rather than 403s) when `target_role` is outside what `caller_role`
/// may manage — the target simply doesn't exist from this caller's point of
/// view, so its existence isn't confirmed/denied any differently than a
/// truly-missing id would be (e.g. a Manager gets the same response
/// whether a given id belongs to another Manager or to no one at all).
fn ensure_manageable(caller_role: Role, target_role: Role) -> AppResult<()> {
    if manageable_roles(caller_role).contains(&target_role) {
        Ok(())
    } else {
        Err(AppError::not_found_with_code(
            "User not found",
            codes::USER_NOT_FOUND,
        ))
    }
}

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
/// roster is small enough to return in full. Always scoped to
/// `manageable_roles(caller_role)` regardless of what `query.role` asks
/// for — a Manager's list is always Staff-only, and an Admin's list never
/// includes the (one) Admin account, matching `get`/`update`/`delete`'s
/// same restriction. Asking for a role outside that scope (e.g. a Manager
/// filtering `role=admin`) returns an empty list rather than an error, the
/// same "just don't show it" spirit as `ensure_manageable`'s 404.
pub(crate) async fn list_users(
    db: &Db,
    query: UserListQuery,
    caller_role: Role,
) -> AppResult<UsersResponse> {
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

    let allowed = manageable_roles(caller_role);
    match query.role {
        Some(role) if allowed.contains(&role) => {
            and_clauses.push(doc! { "role": role.as_str() });
        }
        Some(_) => and_clauses.push(doc! { "role": { "$in": Vec::<String>::new() } }),
        None => {
            let allowed_strs: Vec<&str> = allowed.iter().map(Role::as_str).collect();
            and_clauses.push(doc! { "role": { "$in": allowed_strs } });
        }
    }

    let filter = doc! { "$and": and_clauses };

    let documents = repository::list_users(db, filter).await?;
    let users = documents.into_iter().map(UserDocument::into_user).collect();

    Ok(UsersResponse { users })
}

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

/// Unrestricted account creation. `pub`, not `pub(crate)`, so
/// `src/bin/seed_admin.rs` (a separate binary crate) can create the
/// bootstrap Admin through this exact validation/hashing path instead of
/// duplicating Argon2 logic there — this is also the *only* path an Admin
/// account can ever be created through, since `create_user_for_caller`
/// (what the HTTP route actually calls) never allows `role: Admin` for any
/// caller (see `manageable_roles`). Enforces the single-Admin invariant
/// this deployment assumes: creating a second Admin is rejected outright,
/// including on a `seed_admin` re-run with a different email.
pub async fn create_user(db: &Db, body: CreateUserRequest) -> AppResult<User> {
    if body.name.trim().is_empty() {
        return Err(AppError::validation("Name is required"));
    }
    validate_email(&body.email)?;
    validate_password_strength(&body.password)?;

    if body.role == Role::Admin && repository::count_users_by_role(db, Role::Admin).await? > 0 {
        return Err(AppError::custom(
            StatusCode::CONFLICT,
            codes::ADMIN_ALREADY_EXISTS,
            "An Admin account already exists; this deployment supports only one Admin",
        ));
    }

    let email = normalize_email(&body.email);
    if repository::find_user_by_email(db, &email).await?.is_some() {
        return Err(AppError::conflict(
            codes::EMAIL_ALREADY_EXISTS,
            format!("A user with email '{email}' already exists"),
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
        email,
        password_hash,
        role: body.role,
        is_active: true,
        employee_key: employee_key.clone(),
        created_at: now,
        updated_at: now,
    };

    let inserted = repository::insert_user(db, document).await?;

    if let Some(employee_key) = &employee_key {
        employees::service::touch_by_key(db, employee_key).await?;
    }

    Ok(inserted.into_user())
}

/// Scoped creation for `POST /users` — 403s (not 404, since there's no
/// existing resource to hide) if `body.role` is outside
/// `manageable_roles(caller_role)`, e.g. a Manager trying to create another
/// Manager, or anyone trying to create an Admin.
pub(crate) async fn create_user_for_caller(
    db: &Db,
    body: CreateUserRequest,
    caller_role: Role,
) -> AppResult<User> {
    crate::core::logging::domain::tracked("users.created", async move {
        if !manageable_roles(caller_role).contains(&body.role) {
            return Err(AppError::forbidden_with_code(
                format!(
                    "You are not allowed to create a {} account",
                    body.role.as_str()
                ),
                codes::PERMISSION_DENIED,
            ));
        }
        create_user(db, body).await
    })
    .await
}

/// Partial update for `PATCH /users/{id}` — every field in `body` is
/// optional; required fields (name/email) fall back to the existing
/// document's value before re-validation. `password` present means rehash
/// to the new value; absent leaves the stored hash untouched. Email
/// uniqueness is only re-checked when the email actually changed, so
/// re-saving a user's existing email never trips the uniqueness guard
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

        Ok(updated.into_user())
    })
    .await
}

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

        Ok(deleted.into_user())
    })
    .await
}

/// Cross-module entry point `auth::service::login` calls. "No such email"
/// and "wrong password" both collapse to the same 401
/// `INVALID_CREDENTIALS` (never distinguish which — avoids email
/// enumeration); a correct password against a deactivated account gets its
/// own 401 `USER_INACTIVE`, since at that point the caller has already
/// proven they know the real password.
pub(crate) async fn verify_credentials(db: &Db, email: &str, password: &str) -> AppResult<User> {
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

fn to_login_summary(document: UserDocument) -> EmployeeLoginSummary {
    EmployeeLoginSummary {
        user_id: document
            .id
            .expect("persisted user document must have an id")
            .to_hex(),
        email: document.email,
        role: document.role,
        is_active: document.is_active,
    }
}

/// Cross-module entry point `modules::employees::service` uses to answer
/// "does this employee already have a login" — on create/update validation
/// and on `delete_employee`'s `EMPLOYEE_HAS_LOGIN` guard.
pub(crate) async fn find_user_summary_by_employee_key(
    db: &Db,
    employee_key: &str,
) -> AppResult<Option<EmployeeLoginSummary>> {
    Ok(repository::find_user_by_employee_key(db, employee_key)
        .await?
        .map(to_login_summary))
}

/// Batch variant of `find_user_summary_by_employee_key` — used by
/// `modules::employees::service::list_employees`/`get_employees_by_keys`/
/// `hydrate_sync_documents` to enrich a whole page of employees with their
/// login summary in one query instead of one lookup per row.
pub(crate) async fn find_user_summaries_by_employee_keys(
    db: &Db,
    employee_keys: &[String],
) -> AppResult<HashMap<String, EmployeeLoginSummary>> {
    let documents = repository::find_users_by_employee_keys(db, employee_keys).await?;
    Ok(documents
        .into_iter()
        .filter_map(|document| {
            document
                .employee_key
                .clone()
                .map(|key| (key, to_login_summary(document)))
        })
        .collect())
}
