use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, request::Parts},
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::{Deserialize, Serialize};

use crate::{
    app::AppState,
    core::{
        config::{Config, TenantMode},
        error::{AppError, AppResult},
        middleware::platform_jwt,
        tenancy::Tenant,
    },
    domain::users::Role,
};

/// Claims embedded in the JWT issued at login (`modules::auth::service::login`)
/// and verified on every authenticated request. `role` is optional so
/// tokens without it still decode; `permissions` defaults to empty for the
/// same reason — both matter only to extractors/checks that require them
/// (`AdminUser`, `CurrentUser::require_permission`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    #[serde(default)]
    pub role: Option<Role>,
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Tenant the token was issued for. Absent on single-shop deployments;
    /// required by `CurrentUser` when `TENANT_MODE=multi`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tid: Option<String>,
    /// Space-separated scopes (`device`, `owner`, ...). Set only on tokens
    /// minted for a registered sync device; ordinary logins carry none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Registered device id, present on device tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub did: Option<String>,
}

/// Identifies the caller. Extracting `CurrentUser` from a handler's
/// arguments is what enforces "this route requires auth" — routes that
/// don't take it stay public.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
    pub role: Option<Role>,
    pub permissions: Vec<String>,
    pub tenant_id: Option<String>,
    /// Space-separated scope claim of a platform token (`owner`, `device`,
    /// `staff`); `None` for local HS256 tokens.
    pub scope: Option<String>,
    /// Registered device id (`did`) of a device token.
    pub device_id: Option<String>,
    /// Profile claims of an identity token (`None` for local tokens).
    pub email: Option<String>,
    pub name: Option<String>,
}

impl CurrentUser {
    /// The caller's device id, or 403 `DEVICE_TOKEN_REQUIRED` unless the
    /// token is a device token (`scope` contains `device` and it carries a
    /// `did`). Guards the sync push/pull routes, which only registered
    /// devices - never a web owner session - may call.
    pub fn require_device(&self) -> AppResult<String> {
        let is_device = self
            .scope
            .as_deref()
            .is_some_and(|s| s.split_whitespace().any(|v| v == "device"));
        match (&self.device_id, is_device) {
            (Some(did), true) if !did.is_empty() => Ok(did.clone()),
            _ => Err(AppError::forbidden_with_code(
                "A registered device token is required",
                "DEVICE_TOKEN_REQUIRED",
            )),
        }
    }

    /// 403 `PERMISSION_DENIED` unless `permission` is in this caller's
    /// JWT-embedded permission list. Called as the first line of a
    /// handler body rather than via a dedicated extractor type — Axum's
    /// `FromRequestParts` can't take a runtime constructor argument
    /// without unstable const generics over `&'static str` (only
    /// structural types are stable const generic params), and this
    /// codebase has no existing precedent for parameterized extractors.
    /// This is the pattern to reach for any future permission-gated route.
    pub fn require_permission(&self, permission: &str) -> AppResult<()> {
        if self.permissions.iter().any(|p| p == permission) {
            Ok(())
        } else {
            Err(AppError::forbidden_with_code(
                format!("Missing required permission: {permission}"),
                crate::core::constants::codes::PERMISSION_DENIED,
            ))
        }
    }

    /// Same as `require_permission`, but passes if the caller holds *any*
    /// one of `permissions` — for routes reachable by more than one role
    /// whose access differs only in scope (e.g. `modules::users`, where
    /// `users:manage` and `users:manage:staff` both get past the gate, and
    /// the caller's actual role then determines which accounts they may
    /// touch).
    pub fn require_any_permission(&self, permissions: &[&str]) -> AppResult<()> {
        if permissions
            .iter()
            .any(|wanted| self.permissions.iter().any(|mine| mine == wanted))
        {
            Ok(())
        } else {
            Err(AppError::forbidden_with_code(
                format!("Missing required permission: one of {permissions:?}"),
                crate::core::constants::codes::PERMISSION_DENIED,
            ))
        }
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let verified = verify_bearer(&parts.headers, &state.config).await?;

        if state.config.tenant_mode == TenantMode::Multi && verified.tenant.is_none() {
            return Err(AppError::unauthorized("token has no tenant"));
        }

        let tenant_id = verified.tenant_id();
        Ok(CurrentUser {
            user_id: verified.user_id,
            tenant_id,
            role: verified.role,
            permissions: verified.permissions,
            scope: verified.scope,
            device_id: verified.device_id,
            email: verified.email,
            name: verified.name,
        })
    }
}

/// A bearer token that passed signature and expiry checks, from either issuer.
/// It is the ONE place the tenant of a request is derived (`tenant`), used by
/// both the `CurrentUser` extractor and the tenant middleware, so the tenant a
/// handler is authorised as and the tenant its queries are scoped to cannot
/// disagree.
#[derive(Debug, Clone)]
pub struct VerifiedToken {
    pub user_id: String,
    pub role: Option<Role>,
    pub permissions: Vec<String>,
    /// `None` when the token names no (or an empty) tenant.
    pub tenant: Option<Tenant>,
    pub scope: Option<String>,
    pub device_id: Option<String>,
    /// Profile claims of an identity token (`None` for local tokens).
    pub email: Option<String>,
    pub name: Option<String>,
}

impl VerifiedToken {
    fn new(
        user_id: String,
        role: Option<Role>,
        permissions: Vec<String>,
        tid: Option<String>,
    ) -> Self {
        Self {
            user_id,
            role,
            permissions,
            tenant: tid.and_then(|t| Tenant::id(t).ok()),
            scope: None,
            device_id: None,
            email: None,
            name: None,
        }
    }

    pub fn tenant_id(&self) -> Option<String> {
        match &self.tenant {
            Some(Tenant::Id(id)) => Some(id.to_string()),
            _ => None,
        }
    }
}

/// Verifies the `Authorization: Bearer` token. Dispatches on the token's `alg`
/// header: EdDSA is a platform token (checked against the identity service's
/// JWKS, only when `IDENTITY_JWKS_URL` is configured); anything else takes the
/// local HS256 path unchanged.
pub async fn verify_bearer(headers: &HeaderMap, config: &Config) -> AppResult<VerifiedToken> {
    let header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::unauthorized("missing authorization header"))?;

    let token = header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::unauthorized("expected Bearer token"))?;

    if decode_header(token)?.alg == Algorithm::EdDSA {
        let url = config
            .identity_jwks_url
            .as_deref()
            .ok_or_else(|| AppError::unauthorized("platform tokens are not enabled"))?;
        let cache = platform_jwt::cache_for(url);
        let identity =
            platform_jwt::verify_platform_token(&cache, token, config.identity_issuer.as_deref())
                .await?;
        let mut verified = VerifiedToken::new(
            identity.sub,
            Some(identity.role),
            identity.permissions,
            identity.tid,
        );
        verified.scope = Some(identity.scope);
        verified.email = identity.email;
        verified.name = identity.name;
        verified.device_id = identity.device_id;
        return Ok(verified);
    }

    let decoded = decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::default(),
    )?;
    let mut verified = VerifiedToken::new(
        decoded.claims.sub,
        decoded.claims.role,
        decoded.claims.permissions,
        decoded.claims.tid,
    );
    verified.scope = decoded.claims.scope;
    verified.device_id = decoded.claims.did;
    Ok(verified)
}

/// Identifies a caller whose JWT carries `role: Role::Admin`. This is the
/// pattern to reach for any "must literally be Admin" endpoint (not just
/// inventory's category management) — add `AdminUser` as a handler
/// argument the same way `CurrentUser` gates a route to "any authenticated
/// user", and it rejects with 403 before the handler body runs. For
/// finer-grained gating (e.g. "any role with a specific permission"),
/// prefer `CurrentUser::require_permission` instead.
#[derive(Debug, Clone)]
pub struct AdminUser(pub CurrentUser);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let current_user = CurrentUser::from_request_parts(parts, state).await?;

        if current_user.role != Some(Role::Admin) {
            return Err(AppError::forbidden_with_code(
                "Admin access required",
                crate::core::constants::codes::ADMIN_REQUIRED,
            ));
        }

        Ok(AdminUser(current_user))
    }
}

/// Claims of the short-lived service token the desktop shell mints to call the
/// device-local sync API (`/api/sync/state|outbox|apply|blocks|conflicts`).
#[derive(Debug, Clone, Deserialize)]
struct SyncAgentClaims {
    sub: String,
    scope: String,
}

/// The desktop shell's sync agent, authenticated by an HS256 service token
/// (`sub = "sync-agent"`, `scope = "sync"`, minutes-long expiry) signed with the
/// backend's own `JWT_SECRET`. Ordinary user tokens - and platform tokens - are
/// refused, so only the local shell, which owns that secret, can drive the
/// device's sync state.
#[derive(Debug, Clone)]
pub struct SyncAgent;

impl FromRequestParts<AppState> for SyncAgent {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|header| header.strip_prefix("Bearer "))
            .ok_or_else(|| AppError::unauthorized("missing or malformed authorization header"))?;

        let decoded = decode::<SyncAgentClaims>(
            token,
            &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
            &Validation::new(Algorithm::HS256),
        )?;
        if decoded.claims.scope != "sync" || decoded.claims.sub != "sync-agent" {
            return Err(AppError::forbidden_with_code(
                "A sync service token is required",
                "SYNC_SCOPE_REQUIRED",
            ));
        }
        Ok(SyncAgent)
    }
}
