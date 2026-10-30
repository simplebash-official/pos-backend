use super::parse_object_id::parse_object_id;

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{StockAdjustmentRequest, StockAdjustmentResponse};

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(patch, path = "/products/{id}/stock", tag = modules::INVENTORY,
    params(("id" = String, Path, description = "Product id")),
    request_body = StockAdjustmentRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Stock adjusted", body = ApiResponse<StockAdjustmentResponse>),
        (status = 400, description = "Invalid stock adjustment", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Product not found", body = ErrorResponse),
    )
)]
pub(super) async fn adjust_stock(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<StockAdjustmentRequest>,
) -> AppResult<Json<ApiResponse<StockAdjustmentResponse>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let object_id = parse_object_id(&id)?;
    let response = service::stock::adjust_stock(&state.db, object_id, body).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Stock adjusted successfully",
    )))
}
