// Business rules for login, "who am I", and the login-session audit log.
// Owns no persistence of its own beyond the `login_sessions` collection
// (`super::repository`) — account data is always reached through
// `modules::users::service`, never a Mongo query of this module's own (see
// the module-level comment in `modules::auth::mod`).

use chrono::{Duration, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use mongodb::bson::{Document, doc, oid::ObjectId};

use crate::{
    clients::db::Db,
    core::{
        config::Config,
        constants::{codes, prefixes, roles},
        error::{AppError, AppResult},
        id::generate_id,
        middleware::auth::Claims,
        utils::calculate_pagination,
    },
    domain::{
        auth::{LoginRequest, LoginResponse, LoginSessionListQuery, LoginSessionsResponse},
        users::User,
    },
    modules::{
        auth::{model::LoginSessionDocument, repository},
        users::service as users_service,
    },
};

/// Verifies credentials via `users::service::verify_credentials`, mints a
/// JWT carrying the account's role and its role's default permission set
/// (see `core::constants::roles::default_permissions`), and records the
/// login in the audit log. The audit-log insert is not best-effort — it
/// propagates errors via `?` like everything else in this codebase, so a
/// broken `login_sessions` write blocks login rather than silently losing
/// the record.
pub(crate) async fn login(
    db: &Db,
    config: &Config,
    body: LoginRequest,
    ip_address: Option<String>,
    user_agent: Option<String>,
) -> AppResult<LoginResponse> {
    crate::core::logging::domain::tracked("auth.login", async move {
        if body.email.trim().is_empty() || body.password.is_empty() {
            return Err(AppError::validation("Email and password are required"));
        }

        let user = users_service::verify_credentials(db, &body.email, &body.password).await?;

        let permissions: Vec<String> = roles::default_permissions(user.role)
            .iter()
            .map(|p| p.to_string())
            .collect();
        let exp = (Utc::now() + Duration::hours(config.jwt_expiry_hours)).timestamp() as usize;
        let claims = Claims {
            sub: user.id.clone(),
            exp,
            role: Some(user.role),
            permissions,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
        )?;

        record_login_session(db, &user, ip_address, user_agent).await?;

        Ok(LoginResponse {
            token,
            expires_in: config.jwt_expiry_hours * 3600,
            user,
        })
    })
    .await
}

async fn record_login_session(
    db: &Db,
    user: &User,
    ip_address: Option<String>,
    user_agent: Option<String>,
) -> AppResult<()> {
    let now = mongodb::bson::DateTime::now();
    repository::insert_login_session(
        db,
        LoginSessionDocument {
            id: None,
            key: generate_id(prefixes::LOGIN_SESSION),
            user_key: user.key.clone(),
            name_at_login: user.name.clone(),
            email_at_login: user.email.clone(),
            role_at_login: user.role,
            ip_address,
            user_agent,
            created_at: now,
            updated_at: now,
        },
    )
    .await?;
    Ok(())
}

/// Re-reads the account from Mongo rather than only decoding the JWT — the
/// one place a caller can discover mid-session that their account was
/// deactivated (or deleted) *since* their token was issued, since there is
/// no revocation/blacklist infra to push that information any other way.
pub(crate) async fn me(db: &Db, user_id: &str) -> AppResult<User> {
    let object_id = ObjectId::parse_str(user_id)
        .map_err(|_| AppError::unauthorized("Invalid token subject"))?;

    let user = users_service::get_user(db, object_id)
        .await
        .map_err(|err| match err {
            AppError::NotFound { .. } => {
                AppError::unauthorized_with_code("Account no longer exists", codes::USER_NOT_FOUND)
            }
            other => other,
        })?;

    if !user.is_active {
        return Err(AppError::unauthorized_with_code(
            "This account has been deactivated",
            codes::USER_INACTIVE,
        ));
    }

    Ok(user)
}

/// Backs `GET /auth/sessions` — paginated (unlike `suppliers`' unpaginated
/// lists) since this is an append-only log with unbounded growth. `user_id`
/// filters to one account's `key` when the caller supplies it.
pub(crate) async fn list_sessions(
    db: &Db,
    query: LoginSessionListQuery,
) -> AppResult<LoginSessionsResponse> {
    let (page, limit, skip) = calculate_pagination(query.page, query.limit, 20, 100);

    let filter: Document = match query.user_id {
        Some(user_id) => {
            let object_id = ObjectId::parse_str(&user_id)
                .map_err(|_| AppError::validation("Invalid userId"))?;
            let user = users_service::get_user(db, object_id).await?;
            doc! { "user_key": user.key }
        }
        None => Document::new(),
    };

    let total = repository::count_login_sessions(db, filter.clone()).await?;
    let documents = repository::list_login_sessions(db, filter, skip, limit).await?;
    let sessions = documents
        .into_iter()
        .map(LoginSessionDocument::into_login_session)
        .collect();

    Ok(LoginSessionsResponse {
        sessions,
        total,
        page,
        limit,
    })
}
