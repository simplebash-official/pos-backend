use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::LowStockResponse;

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/products/low-stock", tag = modules::INVENTORY,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Products at or below their minimum stock threshold", body = ApiResponse<LowStockResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn low_stock(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<LowStockResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let response = service::stock::low_stock(&state.db).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Low stock products retrieved successfully",
    )))
}
