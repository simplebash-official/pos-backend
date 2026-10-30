use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{StockMovementListQuery, StockMovementListResponse};

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/stock-movements", tag = modules::INVENTORY, params(StockMovementListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List all stock movements with pagination", body = ApiResponse<StockMovementListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn list_all_stock_movements(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<StockMovementListQuery>,
) -> AppResult<Json<ApiResponse<StockMovementListResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let response = service::stock::list_stock_movements(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Stock movements retrieved successfully",
    )))
}
