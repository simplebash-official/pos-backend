use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::InventoryValuationResponse;

use crate::modules::reports::service;
use axum::Json;

use axum::extract::State;

#[utoipa::path(
    get,
    path = "/inventory-valuation",
    tag = modules::REPORTS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Inventory valuation report at cost vs retail price", body = ApiResponse<InventoryValuationResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_inventory_valuation(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<InventoryValuationResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_inventory_valuation(&state.db).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Inventory valuation report retrieved successfully",
    )))
}
