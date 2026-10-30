use super::authorize::authorize;
use super::reset_selections_request::ResetSelectionsRequest;
use super::reset_selections_response::ResetSelectionsResponse;
use super::to_selections::to_selections;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::{AppError, AppResult};
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::tenants::service;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;

/// Clears a shop's choice of the given design families (and the option values
/// it saved for them): an explicit operator action taken together with
/// revoking the design. Idempotent. Requires `X-Provision-Secret`.
#[utoipa::path(
    post,
    path = "/tenants/{tenant_id}/document-selections/reset",
    tag = modules::TENANTS,
    params(("tenant_id" = String, Path, description = "The identity service's tenant id (`tnt_...`)")),
    request_body = ResetSelectionsRequest,
    responses(
        (status = 200, description = "Choices cleared", body = ApiResponse<ResetSelectionsResponse>),
        (status = 400, description = "No families given", body = ErrorResponse),
        (status = 401, description = "Missing or wrong provisioning secret", body = ErrorResponse),
        (status = 404, description = "Unknown shop, or the endpoint is not enabled on this deployment", body = ErrorResponse),
    )
)]
pub(super) async fn reset_shop_document_selections(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(tenant_id): Path<String>,
    Json(body): Json<ResetSelectionsRequest>,
) -> AppResult<Json<ApiResponse<ResetSelectionsResponse>>> {
    authorize(&state, &headers)?;
    if body.families.is_empty() {
        return Err(AppError::validation(
            "families must name at least one design",
        ));
    }
    let reset =
        service::reset_document_selections(&state.db, tenant_id.trim(), &body.families).await?;
    Ok(Json(ApiResponse::success(
        ResetSelectionsResponse {
            reset: to_selections(reset),
        },
        "Document selections reset",
    )))
}
