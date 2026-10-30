use super::parse_object_id::parse_object_id;

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::StockMovementsResponse;

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/products/{id}/movements", tag = modules::INVENTORY,
    params(("id" = String, Path, description = "Product id")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Stock movement history for a product", body = ApiResponse<StockMovementsResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Product not found", body = ErrorResponse),
    )
)]
pub(super) async fn product_movements(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<StockMovementsResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let object_id = parse_object_id(&id)?;
    let response = service::stock::product_movements(&state.db, object_id).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Stock movements retrieved successfully",
    )))
}
