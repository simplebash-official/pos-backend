use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::InventoryMetrics;

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/stats", tag = modules::INVENTORY,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Inventory & Stock KPI cards: total items, category/subcategory counts, and low-stock alert count", body = ApiResponse<InventoryMetrics>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn inventory_stats(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<InventoryMetrics>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let stats = service::stats::get_inventory_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Inventory stats retrieved successfully",
    )))
}
