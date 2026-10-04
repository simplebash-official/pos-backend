// Puts the caller's tenant in scope for the rest of the request. Runs on every
// request when `TENANT_MODE=multi`: a valid token yields its `tid` as the
// ambient tenant (see `core::tenancy::with_tenant`); anything else - no token,
// a bad token, a token without a tenant - runs under `Tenant::Deny`, so a route
// that is public or forgot its auth extractor still cannot read tenant data.
//
// It also puts the request's sync origin in scope (`core::sync_origin`): the
// registered device of a device token, else the browser's `X-Device-Id`, else
// `cloud`. Every write to a synced collection is stamped with it.

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::{
    app::AppState,
    core::{
        config::TenantMode,
        middleware::auth::verify_bearer,
        sync_origin::{origin_for, with_origin},
        tenancy::{Tenant, with_tenant},
    },
};

pub async fn tenant_context(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if state.config.tenant_mode != TenantMode::Multi {
        return next.run(req).await;
    }
    // Same verification (and same tenant derivation) as the `CurrentUser`
    // extractor; any failure means no tenant, i.e. `Deny`.
    let verified = verify_bearer(req.headers(), &state.config).await.ok();
    let header_device = req
        .headers()
        .get("x-device-id")
        .and_then(|v| v.to_str().ok());
    let origin = origin_for(
        verified.as_ref().and_then(|t| t.device_id.as_deref()),
        header_device,
    );
    let tenant = verified
        .and_then(|token| token.tenant)
        .unwrap_or(Tenant::Deny);
    with_origin(origin, with_tenant(tenant, next.run(req))).await
}
