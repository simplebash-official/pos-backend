use crate::clients::db::Db;
use crate::core::config::Config;
use crate::core::constants::{codes, prefixes, roles};
use crate::core::error::{AppError, AppResult};
use crate::core::id::generate_id;
use crate::core::middleware::auth::Claims;
use crate::core::utils::calculate_pagination;
use crate::domain::auth::{
    ForgotPasswordRequest, LoginRequest, LoginResponse, LoginSessionListQuery,
    LoginSessionsResponse, ShopLookupResponse,
};
use crate::domain::users::{Role, UpdateMyPreferencesRequest, User};
use crate::modules::auth::model::LoginSessionDocument;
use crate::modules::auth::repository;
use crate::modules::sessions;
use crate::modules::tenants::service as tenants_service;
use crate::modules::users::service as users_service;
use chrono::{Duration, Utc};
use jsonwebtoken::{EncodingKey, Header, encode};
use mongodb::bson::oid::ObjectId;
use mongodb::bson::{Document, doc};

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
    device_id: Option<String>,
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

    let result = login_unthrottled(db, config, body, ip_address, user_agent, device_id).await;
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
    device_id: Option<String>,
) -> AppResult<LoginResponse> {
    if config.tenant_mode != crate::core::config::TenantMode::Multi {
        return login_in_scope(db, config, body, ip_address, user_agent, device_id, None).await;
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
        login_in_scope(
            db,
            config,
            body,
            ip_address,
            user_agent,
            device_id,
            Some(shop_name),
        ),
    )
    .await
}

async fn login_in_scope(
    db: &Db,
    config: &Config,
    body: LoginRequest,
    ip_address: Option<String>,
    user_agent: Option<String>,
    device_id: Option<String>,
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

        // In Personal mode with `allowStaffMultiDeviceSessions = false`, staff work from one
        // machine: while their shift is open, no other machine may sign in.
        if user.role == Role::Staff
            && !sessions::service::current_shift_policy(db)
                .await?
                .allows_staff_multi_device()
            && let Some(shift) = sessions::service::resolve_active_session(db, &user.id).await?
            && device_id.as_deref() != Some(shift.terminal_id.as_str())
        {
            return Err(AppError::conflict(
                codes::SESSION_OPEN_ON_OTHER_DEVICE,
                "Your shift is open on another machine. Close it there first.",
            ));
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

/// Backs `PATCH /auth/me/preferences`: resolves the caller exactly like `me`
/// (so identity-server tokens work too), then updates only their own record.
pub(crate) async fn update_my_preferences(
    db: &Db,
    user_id: &str,
    identity_name: Option<&str>,
    changes: UpdateMyPreferencesRequest,
) -> AppResult<User> {
    let current = me(db, user_id, identity_name).await?;
    let object_id = ObjectId::parse_str(&current.id)
        .map_err(|_| AppError::unauthorized("Invalid token subject"))?;
    users_service::update_own_preferences(db, object_id, changes).await
}

fn hash_token(raw_token: &str) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(raw_token.as_bytes());
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

fn execute_dummy_hash() {
    use sha2::{Digest, Sha256};
    let _ = Sha256::digest(b"dummy_entropy_for_constant_time_forgot_password_defense");
}

fn generate_reset_token() -> String {
    use argon2::password_hash::rand_core::{OsRng, RngCore};
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Initiates an Admin password reset request.
pub(crate) async fn forgot_password(
    db: &Db,
    config: &Config,
    req: ForgotPasswordRequest,
) -> AppResult<()> {
    if config.tenant_mode != crate::core::config::TenantMode::Multi {
        return forgot_password_in_scope(db, config, req, None).await;
    }

    let code = req
        .shop_code
        .clone()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if code.is_empty() {
        return Err(AppError::validation("Shop code is required"));
    }

    let details = tenants_service::lookup_shop_details(db, &code).await?;
    let Some((tenant, _)) = details else {
        execute_dummy_hash();
        return Ok(());
    };

    crate::core::tenancy::with_tenant(
        tenant,
        forgot_password_in_scope(db, config, req, Some(code)),
    )
    .await
}

async fn forgot_password_in_scope(
    db: &Db,
    config: &Config,
    req: ForgotPasswordRequest,
    shop_code: Option<String>,
) -> AppResult<()> {
    let trimmed_id = req.identifier.trim();
    if trimmed_id.is_empty() {
        execute_dummy_hash();
        return Ok(());
    }

    let admin_opt = users_service::find_admin_document(db).await?;
    let Some(admin) = admin_opt else {
        execute_dummy_hash();
        return Ok(());
    };

    let username_matches = admin.username.eq_ignore_ascii_case(trimmed_id);
    let admin_email_matches = admin
        .email
        .as_deref()
        .map(str::trim)
        .is_some_and(|e| !e.is_empty() && e.eq_ignore_ascii_case(trimmed_id));

    let tenant_id_str = crate::core::tenancy::current_tenant().and_then(|t| match t {
        crate::core::tenancy::Tenant::Id(id) => Some(id.to_string()),
        _ => None,
    });
    let shop_profile = crate::modules::settings::service::get_shop_profile(db).await?;
    let shop_email_matches = !shop_profile.email.trim().is_empty()
        && shop_profile.email.trim().eq_ignore_ascii_case(trimmed_id);

    if !username_matches && !admin_email_matches && !shop_email_matches {
        execute_dummy_hash();
        return Ok(());
    }

    let recipient_email = admin
        .email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .or_else(|| {
            let e = shop_profile.email.trim();
            if !e.is_empty() { Some(e) } else { None }
        })
        .map(str::to_string);

    let raw_token = generate_reset_token();
    let token_hash = hash_token(&raw_token);

    repository::invalidate_active_tokens_for_user(db, &admin.key).await?;

    let now = mongodb::bson::DateTime::now();
    let expires_at = mongodb::bson::DateTime::from_millis(now.timestamp_millis() + 15 * 60 * 1000);
    let token_key = crate::core::id::generate_id(prefixes::PASSWORD_RESET_TOKEN);

    let doc = crate::modules::auth::model::PasswordResetTokenDocument {
        id: None,
        key: token_key,
        user_key: admin.key.clone(),
        token_hash,
        expires_at,
        used_at: None,
        created_at: now,
        tenant_id: tenant_id_str,
    };
    repository::insert_token(db, doc).await?;

    if let (
        crate::core::config::EmailService::Brevo {
            api_key,
            base_url,
            reset_template_id,
            reset_link_base_url,
        },
        Some(to_email),
    ) = (&config.email_service, recipient_email)
    {
        let reset_link = if let Some(code) = &shop_code {
            format!("{reset_link_base_url}?token={raw_token}&shopCode={code}")
        } else {
            format!("{reset_link_base_url}?token={raw_token}")
        };
        let brevo = crate::clients::brevo::BrevoClient::new(
            base_url.clone(),
            api_key.clone(),
            *reset_template_id,
        );
        let to_name = admin.name.clone();
        let shop_code_clone = shop_code.clone();
        tokio::spawn(async move {
            if let Err(err) = brevo
                .send_password_reset(&to_email, &to_name, &reset_link, shop_code_clone.as_deref())
                .await
            {
                tracing::error!(%err, "failed to send password reset email via Brevo");
            }
        });
    }

    Ok(())
}

async fn find_and_validate_token(
    db: &Db,
    raw_token: &str,
) -> AppResult<crate::modules::auth::model::PasswordResetTokenDocument> {
    if raw_token.trim().is_empty() {
        return Err(AppError::custom(
            axum::http::StatusCode::BAD_REQUEST,
            codes::INVALID_OR_EXPIRED_RESET_TOKEN,
            "Password reset token is invalid or has expired",
        ));
    }
    let token_hash = hash_token(raw_token);
    let token = repository::find_token_by_hash(db, &token_hash)
        .await?
        .ok_or_else(|| {
            AppError::custom(
                axum::http::StatusCode::BAD_REQUEST,
                codes::INVALID_OR_EXPIRED_RESET_TOKEN,
                "Password reset token is invalid or has expired",
            )
        })?;

    if token.used_at.is_some() || token.expires_at.to_chrono() <= Utc::now() {
        return Err(AppError::custom(
            axum::http::StatusCode::BAD_REQUEST,
            codes::INVALID_OR_EXPIRED_RESET_TOKEN,
            "Password reset token is invalid or has expired",
        ));
    }

    Ok(token)
}

/// Verifies that a password reset token exists, has not expired, and has not been used.
pub(crate) async fn verify_reset_token(db: &Db, raw_token: &str) -> AppResult<()> {
    let token = find_and_validate_token(db, raw_token).await?;
    let _ = token;
    Ok(())
}

/// Consumes a valid password reset token and updates the Admin account's password.
pub(crate) async fn reset_password(db: &Db, raw_token: &str, new_password: &str) -> AppResult<()> {
    let token = find_and_validate_token(db, raw_token).await?;

    users_service::validate_password_strength(new_password)?;
    let password_hash = users_service::hash_password(new_password)?;

    repository::mark_token_used(db, &token.key).await?;

    if let Some(tenant_id) = &token.tenant_id {
        let tenant = crate::core::tenancy::Tenant::id(tenant_id)
            .map_err(|e| AppError::internal(format!("invalid tenant id on token: {e}")))?;
        crate::core::tenancy::with_tenant(tenant, async {
            let admin = users_service::find_admin_document(db)
                .await?
                .ok_or_else(|| {
                    AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND)
                })?;
            let admin_id = admin.id.expect("persisted admin document must have an id");
            users_service::update_admin_password_and_bump_version(db, admin_id, &password_hash)
                .await
        })
        .await?;
    } else {
        let admin = users_service::find_admin_document(db)
            .await?
            .ok_or_else(|| {
                AppError::not_found_with_code("User not found", codes::USER_NOT_FOUND)
            })?;
        let admin_id = admin.id.expect("persisted admin document must have an id");
        users_service::update_admin_password_and_bump_version(db, admin_id, &password_hash).await?;
    }

    Ok(())
}
