use axum::{extract::FromRequestParts, http::request::Parts};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};

use crate::{
    app::AppState,
    core::error::{AppError, AppResult},
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
}

/// Identifies the caller. Extracting `CurrentUser` from a handler's
/// arguments is what enforces "this route requires auth" — routes that
/// don't take it stay public.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
    pub role: Option<Role>,
    pub permissions: Vec<String>,
}

impl CurrentUser {
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
            permissions: decoded.claims.permissions,
        })
    }
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
