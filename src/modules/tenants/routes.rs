// HTTP surface of the tenant directory: the one server-to-server endpoint the
// identity service calls when a shop registers. Not a browser API — it is
// authenticated by a shared secret (never a bearer token) and nginx refuses to
// forward `/api/internal/` from the internet, so only containers on the compose
// network can reach it.

use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        config::TenantMode,
        constants::modules,
        error::{AppError, AppResult},
        response::{ApiResponse, ErrorResponse},
    },
    modules::tenants::{
        service::{self, ProvisionOwner},
        usage::{self, TenantUsage},
    },
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(provision_shop))
        .routes(routes!(set_status))
        .routes(routes!(shop_usage))
        .routes(routes!(batch_usage))
}

// ============================================================================
// Helpers
// ============================================================================

const PROVISION_SECRET_HEADER: &str = "x-provision-secret";

/// Compares without an early exit on the first differing byte, so response time
/// doesn't reveal how much of a guessed secret was right.
fn secrets_match(provided: &[u8], expected: &[u8]) -> bool {
    let mut diff = provided.len() ^ expected.len();
    for i in 0..provided.len().max(expected.len()) {
        let a = provided.get(i).copied().unwrap_or(0);
        let b = expected.get(i).copied().unwrap_or(0);
        diff |= usize::from(a ^ b);
    }
    diff == 0
}

/// The endpoint only exists in a multi-tenant deployment that configured a
/// secret; anywhere else it answers 404 as if it were not routed at all.
fn authorize(state: &AppState, headers: &HeaderMap) -> AppResult<()> {
    let expected = match (
        state.config.tenant_mode,
        state.config.provision_secret.as_deref(),
    ) {
        (TenantMode::Multi, Some(secret)) => secret,
        _ => return Err(AppError::not_found("Not found")),
    };
    let provided = headers
        .get(PROVISION_SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if secrets_match(provided.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(AppError::unauthorized("Invalid provisioning secret"))
    }
}

// ============================================================================
// Shop provisioning
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionOwnerRequest {
    /// Always `admin`; the shop code + this username is the POS login.
    pub username: String,
    pub name: String,
    /// Argon2id PHC string of the admin password the owner chose for this shop (`$argon2id$...`), never a plaintext password.
    pub password_hash: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionShopRequest {
    /// The identity service's tenant id (`tnt_...`).
    pub tenant_id: String,
    pub shop_code: String,
    pub name: String,
    /// Present for web sign-ups; absent for desktop ones (their admin arrives via device sync).
    pub owner: Option<ProvisionOwnerRequest>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionShopResponse {
    /// False when the shop was already registered (a replay).
    pub tenant_created: bool,
    /// False when the shop already had an Admin, or no owner was supplied.
    pub admin_created: bool,
}

/// Registers a shop created by the identity service and, optionally, its first
/// Admin. Idempotent; requires the shared `X-Provision-Secret` header.
#[utoipa::path(
    post,
    path = "/provision",
    tag = modules::TENANTS,
    request_body = ProvisionShopRequest,
    responses(
        (status = 200, description = "Shop registered (or already registered)", body = ApiResponse<ProvisionShopResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Provisioning is not enabled on this deployment", body = ErrorResponse),
        (status = 409, description = "Shop code or tenant id already used by a different shop", body = ErrorResponse),
    )
)]
async fn provision_shop(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ProvisionShopRequest>,
) -> AppResult<Json<ApiResponse<ProvisionShopResponse>>> {
    authorize(&state, &headers)?;

    let owner = body.owner.map(|o| ProvisionOwner {
        username: o.username,
        name: o.name,
        password_hash: o.password_hash,
    });
    let outcome = service::provision_shop(
        &state.db,
        body.tenant_id.trim(),
        &body.shop_code,
        &body.name,
        owner,
    )
    .await?;

    Ok(Json(ApiResponse::success(
        ProvisionShopResponse {
            tenant_created: outcome.tenant_created,
            admin_created: outcome.admin_created,
        },
        "Shop provisioned",
    )))
}

// ============================================================================
// Suspension and usage (operator console)
// ============================================================================

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetStatusRequest {
    /// True switches the shop's cloud access off; false switches it back on.
    pub suspended: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub tenant_id: String,
    pub suspended: bool,
}

/// Suspends or reinstates a shop. While suspended, every request carrying a token of the shop is
/// refused with 403 `ACCOUNT_SUSPENDED` (within about 30 s on every instance) and shop-code
/// logins are refused after the password check. Idempotent; requires `X-Provision-Secret`.
#[utoipa::path(
    put,
    path = "/tenants/{tenant_id}/status",
    tag = modules::TENANTS,
    params(("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)")),
    request_body = SetStatusRequest,
    responses(
        (status = 200, description = "Status applied", body = ApiResponse<StatusResponse>),
        (status = 400, description = "Malformed tenant id", body = ErrorResponse),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
async fn set_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Json(body): Json<SetStatusRequest>,
) -> AppResult<Json<ApiResponse<StatusResponse>>> {
    authorize(&state, &headers)?;
    let tenant_id = tenant_id.trim().to_string();
    service::set_suspended(&state.db, &tenant_id, body.suspended).await?;
    Ok(Json(ApiResponse::success(
        StatusResponse {
            tenant_id,
            suspended: body.suspended,
        },
        if body.suspended {
            "Shop suspended"
        } else {
            "Shop reinstated"
        },
    )))
}

/// How much one shop uses the POS: bill counts, gross sales, catalogue and staff sizes, devices
/// and the last sync. Counts and totals only, no record content. Requires `X-Provision-Secret`.
#[utoipa::path(
    get,
    path = "/tenants/{tenant_id}/usage",
    tag = modules::TENANTS,
    params(("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)")),
    responses(
        (status = 200, description = "Usage figures", body = ApiResponse<TenantUsage>),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
async fn shop_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
) -> AppResult<Json<ApiResponse<TenantUsage>>> {
    authorize(&state, &headers)?;
    let usage = usage::usage_for(&state.db, tenant_id.trim()).await?;
    Ok(Json(ApiResponse::success(usage, "Shop usage")))
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchUsageRequest {
    /// Up to 100 tenant ids; ids that are not in the directory are left out of the answer.
    pub tenant_ids: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchUsageResponse {
    pub items: Vec<TenantUsage>,
}

/// Usage of several shops in one call (the console's lists). Requires `X-Provision-Secret`.
#[utoipa::path(
    post,
    path = "/tenants/usage",
    tag = modules::TENANTS,
    request_body = BatchUsageRequest,
    responses(
        (status = 200, description = "Usage figures", body = ApiResponse<BatchUsageResponse>),
        (status = 400, description = "More than 100 shops", body = ErrorResponse),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "The endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
async fn batch_usage(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BatchUsageRequest>,
) -> AppResult<Json<ApiResponse<BatchUsageResponse>>> {
    authorize(&state, &headers)?;
    let ids: Vec<String> = body
        .tenant_ids
        .iter()
        .map(|i| i.trim().to_string())
        .collect();
    let items = usage::usage_for_many(&state.db, &ids).await?;
    Ok(Json(ApiResponse::success(
        BatchUsageResponse { items },
        "Shop usage",
    )))
}

#[cfg(test)]
mod tests {
    use super::secrets_match;

    #[test]
    fn secrets_match_only_when_identical() {
        assert!(secrets_match(b"abcdef", b"abcdef"));
        assert!(!secrets_match(b"abcdef", b"abcdeg"));
        assert!(!secrets_match(b"abcde", b"abcdef"));
        assert!(!secrets_match(b"abcdefg", b"abcdef"));
        assert!(!secrets_match(b"", b"abcdef"));
    }
}
