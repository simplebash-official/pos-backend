use axum::{extract::FromRequestParts, http::request::Parts};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

use crate::{app::AppState, core::error::AppError};

/// Claims embedded in the JWT issued at login and verified on every
/// authenticated request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

/// Identifies the caller. Extracting `CurrentUser` from a handler's
/// arguments is what enforces "this route requires auth" — routes that
/// don't take it stay public.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("missing authorization header".to_string()))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("expected Bearer token".to_string()))?;

        let decoded = decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )?;

        Ok(CurrentUser {
            user_id: decoded.claims.sub,
        })
    }
}
