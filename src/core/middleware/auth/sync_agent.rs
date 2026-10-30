use crate::app::AppState;

use crate::core::error::AppError;

use axum::extract::FromRequestParts;

use axum::http::request::Parts;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;

/// Claims of the short-lived service token the desktop shell mints to call the
/// device-local sync API (`/api/sync/state|outbox|apply|blocks|conflicts`).
#[derive(Debug, Clone, Deserialize)]
pub(super) struct SyncAgentClaims {
    pub(super) sub: String,
    pub(super) scope: String,
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
