//! Puts the caller's tenant in scope for the rest of the request. Runs on every
//! request when `TENANT_MODE=multi`: a valid token yields its `tid` as the
//! ambient tenant (see `core::tenancy::with_tenant`); anything else - no token,
//! a bad token, a token without a tenant - runs under `Tenant::Deny`, so a route
//! that is public or forgot its auth extractor still cannot read tenant data.
//!
//! It also puts the request's sync origin in scope (`core::sync_origin`): the
//! registered device of a device token, else the browser's `X-Device-Id`, else
//! `cloud`. Every write to a synced collection is stamped with it.

use axum::extract::{Request, State};

use axum::middleware::Next;

use axum::response::{IntoResponse, Response};

use crate::app::AppState;

use crate::core::config::TenantMode;

use crate::core::middleware::auth::verify_bearer;

use crate::core::sync_origin::{origin_for, with_origin};

use crate::core::tenancy::{Tenant, with_tenant};

/// Middleware that puts the caller's tenant in scope for the rest of the request.
pub async fn tenant_context(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if state.config.tenant_mode != TenantMode::Multi {
        return next.run(req).await;
    }
    // Same verification (and same tenant derivation) as the `CurrentUser`
    // extractor; any failure means no tenant, i.e. `Deny`.
    let verified = match verify_bearer(req.headers(), &state.config).await {
        Ok(verified) => Some(verified),
        Err(_) => None,
    };
    let header_device = req
        .headers()
        .get("x-device-id")
        .and_then(|v| v.to_str().ok());
    let origin = origin_for(
        verified.as_ref().and_then(|t| t.device_id.as_deref()),
        header_device,
    );
    // A suspended shop is refused here, once, for every route and every token type, before any
    // handler runs. The check is cached (30 s) and fails open: a directory read error must not
    // become a second way for every request to fail, since the handler's own queries would fail too.
    if let Some(tenant_id) = verified.as_ref().and_then(|token| token.tenant_id()) {
        match crate::modules::tenants::service::suspension_of(&state.db, &tenant_id).await {
            Ok(Some(info)) => {
                return crate::modules::tenants::service::suspended_error(&info).into_response();
            }
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(%tenant_id, error = %err, "could not read the shop's suspension flag")
            }
        }
    }
    let tenant = match verified.and_then(|token| token.tenant) {
        Some(tenant) => tenant,
        None => Tenant::Deny,
    };
    with_origin(origin, with_tenant(tenant, next.run(req))).await
}
