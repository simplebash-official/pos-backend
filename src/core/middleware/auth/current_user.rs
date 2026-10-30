use super::account_status::ACCOUNT_STATUS;
use super::verified_token::VerifiedToken;
use super::verify_bearer::verify_bearer;
use crate::app::AppState;
use crate::core::config::TenantMode;
use crate::core::error::{AppError, AppResult};

use crate::domain::users::Role;
use axum::extract::FromRequestParts;

use axum::http::request::Parts;

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
    /// Resolves the human-readable display name for this user.
    /// Checks the token's `name` claim first; falls back to querying the user
    /// by their ObjectId in `db`; falls back to "Cashier" if neither is available.
    pub async fn resolve_name(&self, db: &crate::clients::db::Db) -> String {
        if let Some(ref name) = self.name {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        if let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(&self.user_id)
            && let Ok(user) = crate::modules::users::service::get_user(db, oid).await
        {
            let trimmed = user.name.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        "Cashier".to_string()
    }

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
        if self.has_permission(permission) {
            Ok(())
        } else {
            Err(AppError::forbidden_with_code(
                format!("Missing required permission: {permission}"),
                crate::core::constants::codes::PERMISSION_DENIED,
            ))
        }
    }

    /// Whether the caller holds `permission`, for handlers whose behaviour (not
    /// access) depends on it, e.g. which movement types a category may be made for.
    #[must_use]
    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p == permission)
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

        ensure_account_still_valid(state, &verified).await?;

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

/// How long a looked-up account status is trusted before the next request
/// re-reads it. Changes made through this API invalidate it immediately
/// (`invalidate_account_status`); anything else (a sync push, a restore) is
/// picked up within this window.
const ACCOUNT_STATUS_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// A JWT stays valid until it expires, so without this a deactivated,
/// deleted or demoted user would keep their old access for up to
/// `JWT_EXPIRY_HOURS`. Applies to ordinary local logins (HS256 tokens whose
/// subject is a user id). Device and identity-service tokens carry a `scope`
/// and are governed by the device registry / identity service instead.
async fn ensure_account_still_valid(state: &AppState, verified: &VerifiedToken) -> AppResult<()> {
    if verified.scope.is_some() || verified.device_id.is_some() {
        return Ok(());
    }
    let Ok(oid) = mongodb::bson::oid::ObjectId::parse_str(&verified.user_id) else {
        // Not a user-account subject (e.g. a service identity).
        return Ok(());
    };
    let cache_key = format!(
        "{}|{}",
        verified.tenant_id().unwrap_or_default(),
        verified.user_id
    );

    let cached = ACCOUNT_STATUS
        .lock()
        .expect("account status cache mutex poisoned")
        .get(&cache_key)
        .filter(|(_, read_at)| read_at.elapsed() < ACCOUNT_STATUS_TTL)
        .map(|(status, _)| *status);
    let status = match cached {
        Some(status) => status,
        None => {
            let status = match crate::modules::users::service::get_user(&state.db, oid).await {
                Ok(user) => Some((user.is_active, user.role)),
                Err(AppError::NotFound { .. }) => None,
                Err(err) => return Err(err),
            };
            ACCOUNT_STATUS
                .lock()
                .expect("account status cache mutex poisoned")
                .insert(cache_key, (status, std::time::Instant::now()));
            status
        }
    };

    match status {
        None => Err(AppError::unauthorized_with_code(
            "This account no longer exists. Please sign in again.",
            crate::core::constants::codes::SESSION_REVOKED,
        )),
        Some((false, _)) => Err(AppError::unauthorized_with_code(
            "This account has been deactivated",
            crate::core::constants::codes::USER_INACTIVE,
        )),
        Some((true, role)) if verified.role.is_some_and(|r| r != role) => {
            Err(AppError::unauthorized_with_code(
                "Your access has changed. Please sign in again.",
                crate::core::constants::codes::SESSION_REVOKED,
            ))
        }
        Some(_) => Ok(()),
    }
}
