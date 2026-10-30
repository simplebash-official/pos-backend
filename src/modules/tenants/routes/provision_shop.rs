use super::authorize::authorize;
use super::provision_shop_request::ProvisionShopRequest;
use super::provision_shop_response::ProvisionShopResponse;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::modules::tenants::service::ProvisionOwner;

use crate::modules::tenants::service;
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;

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
pub(super) async fn provision_shop(
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
