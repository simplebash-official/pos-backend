// HTTP surface of the tenant directory: the one server-to-server endpoint the
// identity service calls when a shop registers. Not a browser API — it is
// authenticated by a shared secret (never a bearer token) and nginx refuses to
// forward `/api/internal/` from the internet, so only containers on the compose
// network can reach it.

use axum::{Json, extract::State, http::HeaderMap};
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
    modules::tenants::service::{self, ProvisionOwner},
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(provision_shop))
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
    pub email: String,
    pub name: String,
    /// The account's Argon2id PHC string (`$argon2id$...`), never a plaintext password.
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
        email: o.email,
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
