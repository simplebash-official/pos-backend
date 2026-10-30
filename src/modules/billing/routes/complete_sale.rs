use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::billing::{CompleteSaleResponse, CreateSaleRequest};

use crate::modules::billing::service::sale;

use axum::Json;

use axum::extract::State;

#[utoipa::path(post, path = "/sales", tag = modules::BILLING, request_body = CreateSaleRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Sale completed — invoice and any payment(s) recorded", body = ApiResponse<CompleteSaleResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "customerKey does not resolve to an existing customer", body = ErrorResponse),
    )
)]
pub(super) async fn complete_sale(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Json(body): Json<CreateSaleRequest>,
) -> AppResult<Json<ApiResponse<CompleteSaleResponse>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let response = sale::complete_sale(&state.db, body, user.user_id, device_id.0).await?;
    state.reports_engine.invalidate_active();
    Ok(Json(ApiResponse::success(
        response,
        "Sale completed successfully",
    )))
}
