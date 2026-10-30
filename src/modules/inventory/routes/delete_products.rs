use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{DeleteProductsRequest, DeleteProductsResponse};

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(delete, path = "/products", tag = modules::INVENTORY, request_body = DeleteProductsRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Products deleted", body = ApiResponse<DeleteProductsResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn delete_products(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<DeleteProductsRequest>,
) -> AppResult<Json<ApiResponse<DeleteProductsResponse>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let deleted_count = service::product::delete_products(&state.db, body.product_ids).await?;

    Ok(Json(ApiResponse::success(
        DeleteProductsResponse { deleted_count },
        format!("{deleted_count} products deleted successfully"),
    )))
}
