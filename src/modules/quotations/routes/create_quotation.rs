use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::quotations::{CreateQuotationRequest, Quotation};

use crate::modules::quotations::service;
use axum::Json;

use axum::extract::State;

#[utoipa::path(post, path = "/", tag = modules::QUOTATIONS, request_body = CreateQuotationRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Quotation created", body = ApiResponse<Quotation>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Customer or product not found", body = ErrorResponse),
    )
)]
pub(super) async fn create_quotation(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Json(body): Json<CreateQuotationRequest>,
) -> AppResult<Json<ApiResponse<Quotation>>> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let quotation =
        service::create_quotation(&state.db, user.tenant_id.as_deref(), body, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        quotation,
        "Quotation created successfully",
    )))
}
