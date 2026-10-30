use super::jwks_cache::JwksCache;
use super::platform_identity::PlatformIdentity;
use super::role_for_scope::role_for_scope;
use crate::core::constants::roles;
use crate::core::error::{AppError, AppResult};

use jsonwebtoken::{Algorithm, Validation, decode, decode_header};
use serde::Deserialize;

/// Claims of a platform access token.
#[derive(Debug, Deserialize)]
pub(super) struct PlatformClaims {
    /// Account id of the caller.
    pub(super) sub: String,
    /// Tenant (business) the token is valid for.
    #[serde(default)]
    pub(super) tid: Option<String>,
    /// Space-separated scope values, e.g. `owner` or `device`.
    #[serde(default)]
    pub(super) scope: String,
    /// Registered device the token was issued to (device tokens only).
    #[serde(default)]
    pub(super) did: Option<String>,
    /// The account's email and display name (interactive identity tokens only).
    #[serde(default)]
    pub(super) email: Option<String>,
    #[serde(default)]
    pub(super) name: Option<String>,
}

/// Verifies an EdDSA platform token: signature against the JWKS key named by
/// the `kid` header, `exp`, and (when configured) `iss`.
pub async fn verify_platform_token(
    cache: &JwksCache,
    token: &str,
    issuer: Option<&str>,
) -> AppResult<PlatformIdentity> {
    let header = decode_header(token)?;
    if header.alg != Algorithm::EdDSA {
        return Err(AppError::unauthorized("unsupported token algorithm"));
    }
    let kid = header
        .kid
        .ok_or_else(|| AppError::unauthorized("token has no key id"))?;
    let key = cache
        .key(&kid)
        .await
        .ok_or_else(|| AppError::unauthorized("unknown token signing key"))?;

    let mut validation = Validation::new(Algorithm::EdDSA);
    if let Some(iss) = issuer {
        validation.set_issuer(&[iss]);
    }
    let claims = decode::<PlatformClaims>(token, &key, &validation)?.claims;

    let role = role_for_scope(&claims.scope)
        .ok_or_else(|| AppError::unauthorized("token scope is not permitted"))?;
    // An account session without a shop names no tenant to be Admin of.
    let only_account = claims.scope.split_whitespace().all(|s| s == "account");
    if only_account && claims.tid.as_deref().is_none_or(str::is_empty) {
        return Err(AppError::unauthorized("account token has no shop"));
    }
    Ok(PlatformIdentity {
        sub: claims.sub,
        tid: claims.tid,
        scope: claims.scope.clone(),
        device_id: claims.did,
        email: claims.email,
        name: claims.name,
        role,
        permissions: roles::default_permissions(role)
            .iter()
            .map(|p| p.to_string())
            .collect(),
    })
}
