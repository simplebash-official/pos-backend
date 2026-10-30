use crate::clients::db::Db;
use crate::core::config::Config;
use crate::core::constants::{prefixes, roles};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::middleware::auth::Claims;
use crate::domain::auth::{LoginRequest, LoginResponse};
use crate::domain::users::User;
use crate::modules::auth::model::LoginSessionDocument;
use crate::modules::auth::repository;
use crate::modules::tenants::service as tenants_service;
use crate::modules::users::service as users_service;
use chrono::{Duration, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};

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
        body.username.trim().to_lowercase()
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
        Err(err) if err.wire_parts().status == axum::http::StatusCode::UNAUTHORIZED => {
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
        .ok_or_else(|| AppError::unauthorized("Invalid shop code, username or password"))?;
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
        if body.username.trim().is_empty() || body.password.is_empty() {
            return Err(AppError::validation("Username and password are required"));
        }

        let user = users_service::verify_credentials(db, &body.username, &body.password).await?;

        // Only someone who proved the password learns the shop is suspended (shop codes are
        // semi-public, so this must not reveal it to a stranger).
        if shop_name.is_some()
            && let Some(tenant_id) = crate::core::tenancy::current_tenant_id()
            && let Some(info) = tenants_service::suspension_of(db, &tenant_id).await?
        {
            return Err(tenants_service::suspended_error(&info));
        }

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
            name: Some(user.name.clone()),
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
            username_at_login: user.username.clone(),
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
