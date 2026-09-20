// Puts the caller's tenant in scope for the rest of the request. Runs on every
// request when `TENANT_MODE=multi`: a valid token yields its `tid` as the
// ambient tenant (see `core::tenancy::with_tenant`); anything else - no token,
// a bad token, a token without a tenant - runs under `Tenant::Deny`, so a route
// that is public or forgot its auth extractor still cannot read tenant data.

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
        tenancy::{Tenant, with_tenant},
    },
};

pub async fn tenant_context(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if state.config.tenant_mode != TenantMode::Multi {
        return next.run(req).await;
    }
    // Same verification (and same tenant derivation) as the `CurrentUser`
    // extractor; any failure means no tenant, i.e. `Deny`.
    let tenant = verify_bearer(req.headers(), &state.config)
        .await
        .ok()
        .and_then(|token| token.tenant)
        .unwrap_or(Tenant::Deny);
    with_tenant(tenant, next.run(req)).await
}
