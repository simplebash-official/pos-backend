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
        auth::{
            LoginRequest, LoginResponse, LoginSessionListQuery, LoginSessionsResponse,
            ShopLookupResponse,
        },
        users::{UpdateMyPreferencesRequest, User},
    },
    modules::{
        auth::{model::LoginSessionDocument, repository},
        tenants::service as tenants_service,
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
    // Brute-force guard, per shop + account (see `core::rate_limit`). Checked
    // before the password hash is touched, so a blocked account also stops
    // costing an Argon2 verification per attempt.
    let limiter = &*crate::core::rate_limit::LOGIN_FAILURES;
    let limit_key = format!(
        "{}|{}",
        body.shop_code
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_lowercase(),
        body.email.trim().to_lowercase()
    );
    if let Err(retry_after) = limiter.check(&limit_key) {
        let minutes = retry_after.as_secs().div_ceil(60).max(1);
        return Err(AppError::custom(
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            crate::core::constants::codes::TOO_MANY_LOGIN_ATTEMPTS,
            format!("Too many failed sign-in attempts. Please try again in {minutes} minute(s)."),
        ));
    }

    let result = login_unthrottled(db, config, body, ip_address, user_agent).await;
    match &result {
        Ok(_) => limiter.reset(&limit_key),
        Err(err)
            if err.status_code_code_and_message().0 == axum::http::StatusCode::UNAUTHORIZED =>
        {
            limiter.record_failure(&limit_key)
        }
        Err(_) => {}
    }
    result
}

async fn login_unthrottled(
    db: &Db,
    config: &Config,
    body: LoginRequest,
    ip_address: Option<String>,
    user_agent: Option<String>,
) -> AppResult<LoginResponse> {
    if config.tenant_mode != crate::core::config::TenantMode::Multi {
        return login_in_scope(db, config, body, ip_address, user_agent, None).await;
    }

    // Multi-tenant: the shop code picks the tenant, and everything below - the
    // credential check, the session record and the token's `tid` - runs inside
    // that tenant. An unknown code and a wrong password look identical, so the
    // endpoint cannot be used to enumerate shops.
    let code = body.shop_code.as_deref().map(str::trim).unwrap_or_default();
    if code.is_empty() {
        return Err(AppError::validation("Shop code is required"));
    }
    let (tenant, shop_name) = tenants_service::lookup_shop_details(db, code)
        .await?
        .ok_or_else(|| AppError::unauthorized("Invalid shop code, email or password"))?;
    crate::core::tenancy::with_tenant(
        tenant,
        login_in_scope(db, config, body, ip_address, user_agent, Some(shop_name)),
    )
    .await
}

async fn login_in_scope(
    db: &Db,
    config: &Config,
    body: LoginRequest,
    ip_address: Option<String>,
    user_agent: Option<String>,
    shop_name: Option<String>,
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
            tid: crate::core::tenancy::current_tenant_id(),
            scope: None,
            did: None,
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
            shop_name,
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
pub(crate) async fn me(
    db: &Db,
    user_id: &str,
    identity_email: Option<&str>,
    identity_name: Option<&str>,
) -> AppResult<User> {
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
        // ObjectId: find the shop's user for that account by its email claim.
        Err(_) => match identity_email {
            Some(email) => users_service::find_user_for_identity(db, email, identity_name)
                .await?
                .ok_or_else(gone)?,
            None => return Err(AppError::unauthorized("Invalid token subject")),
        },
    };

    if !user.is_active {
        return Err(AppError::unauthorized_with_code(
            "This account has been deactivated",
            codes::USER_INACTIVE,
        ));
    }

    Ok(user)
}

/// Backs `PATCH /auth/me/preferences`: resolves the caller exactly like `me`
/// (so identity-server tokens work too), then updates only their own record.
pub(crate) async fn update_my_preferences(
    db: &Db,
    user_id: &str,
    identity_email: Option<&str>,
    identity_name: Option<&str>,
    changes: UpdateMyPreferencesRequest,
) -> AppResult<User> {
    let current = me(db, user_id, identity_email, identity_name).await?;
    let object_id = ObjectId::parse_str(&current.id)
        .map_err(|_| AppError::unauthorized("Invalid token subject"))?;
    users_service::update_own_preferences(db, object_id, changes).await
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

/// Looks up a shop by its code and returns its display name (for branding the login screen).
pub(crate) async fn lookup_shop(db: &Db, code: &str) -> AppResult<ShopLookupResponse> {
    let (_, name) = tenants_service::lookup_shop_details(db, code)
        .await?
        .ok_or_else(|| AppError::not_found(format!("No shop found with code '{}'", code.trim())))?;
    Ok(ShopLookupResponse {
        shop_code: code.trim().to_ascii_lowercase(),
        name,
    })
}
