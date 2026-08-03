use axum::{extract::FromRequestParts, http::request::Parts};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

use crate::{app::AppState, core::error::AppError};

/// Claims embedded in the JWT issued at login and verified on every
/// authenticated request. `role` is optional so existing tokens without it
/// still decode; it's only checked by extractors (like `AdminUser`) that
/// require a specific role.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    #[serde(default)]
    pub role: Option<String>,
}

/// Identifies the caller. Extracting `CurrentUser` from a handler's
/// arguments is what enforces "this route requires auth" — routes that
/// don't take it stay public.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
    pub role: Option<String>,
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
            .ok_or_else(|| AppError::unauthorized("missing authorization header"))?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::unauthorized("expected Bearer token"))?;

        let decoded = decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.config.jwt_secret.as_bytes()),
            &Validation::default(),
        )?;

        Ok(CurrentUser {
            user_id: decoded.claims.sub,
            role: decoded.claims.role,
        })
    }
}

/// Identifies a caller whose JWT carries `role: "admin"`. This is the
/// pattern to reach for any admin-only endpoint (not just inventory's
/// category management) — add `AdminUser` as a handler argument the same
/// way `CurrentUser` gates a route to "any authenticated user", and it
/// rejects with 403 before the handler body runs.
#[derive(Debug, Clone)]
pub struct AdminUser(pub CurrentUser);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let current_user = CurrentUser::from_request_parts(parts, state).await?;

        if current_user.role.as_deref() != Some("admin") {
            return Err(AppError::forbidden_with_code(
                "Admin access required",
                crate::core::constants::codes::ADMIN_REQUIRED,
            ));
        }

        Ok(AdminUser(current_user))
    }
}
