use super::claims::Claims;
use super::verified_token::VerifiedToken;

use crate::core::config::{Config, TenantMode};
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::platform_jwt;

use axum::http::HeaderMap;

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};

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
        ensure_platform_token_is_for_this_shop(config, identity.tid.as_deref())?;
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
    verified.name = decoded.claims.name;
    Ok(verified)
}

/// A single-shop deployment serves exactly one shop, but the identity service
/// signs tokens for every shop it knows. Without this check any account
/// registered there (scopes `owner`/`account`/`device` map to Admin) would be
/// Admin here. Multi-tenant deployments are covered instead by the tenant
/// scoping every query gets from the token's `tid`.
fn ensure_platform_token_is_for_this_shop(config: &Config, tid: Option<&str>) -> AppResult<()> {
    if config.tenant_mode == TenantMode::Multi {
        return Ok(());
    }
    match (config.identity_tenant_id.as_deref(), tid) {
        (Some(expected), Some(actual)) if !expected.is_empty() && expected == actual => Ok(()),
        (None, _) => Err(AppError::unauthorized(
            "platform tokens are not accepted by this shop (IDENTITY_TENANT_ID is not set)",
        )),
        _ => Err(AppError::unauthorized("token is not for this shop")),
    }
}
