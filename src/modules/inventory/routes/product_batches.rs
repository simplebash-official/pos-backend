use super::batch_for::batch_for;

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::StockBatchListResponse;

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/products/{key}/batches", tag = modules::INVENTORY,
    params(("key" = String, Path, description = "Product key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Every stock batch of the product, newest first (unitCostCents only for Admin)", body = ApiResponse<StockBatchListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Product not found", body = ErrorResponse),
    )
)]
pub(super) async fn product_batches(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<StockBatchListResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let mut response = service::stock_batch::list_batches(&state.db, &key).await?;
    response.items = response
        .items
        .into_iter()
        .map(|batch| batch_for(&user, batch))
        .collect();

    Ok(Json(ApiResponse::success(
        response,
        "Stock batches retrieved successfully",
    )))
}
