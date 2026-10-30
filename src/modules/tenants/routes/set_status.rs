use super::authorize::authorize;
use super::set_status_request::SetStatusRequest;
use super::status_response::StatusResponse;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::tenants::service;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;

/// Suspends or reinstates a shop. While suspended, every request carrying a token of the shop is
/// refused with 403 `ACCOUNT_SUSPENDED` carrying `details` `{suspendedAt, reason, retained}`
/// (within about 30 s on every instance) and shop-code
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
pub(super) async fn set_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Json(body): Json<SetStatusRequest>,
) -> AppResult<Json<ApiResponse<StatusResponse>>> {
    authorize(&state, &headers)?;
    let tenant_id = tenant_id.trim().to_string();
    service::set_suspended(&state.db, &tenant_id, body.suspension()?).await?;
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
