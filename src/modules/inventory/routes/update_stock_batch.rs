use super::batch_for::batch_for;

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{StockBatch, UpdateStockBatchRequest};
use crate::domain::users::Role;
use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(patch, path = "/batches/{key}", tag = modules::INVENTORY,
    params(("key" = String, Path, description = "Stock batch key")),
    request_body = UpdateStockBatchRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Batch price/cost updated", body = ApiResponse<StockBatch>),
        (status = 400, description = "Invalid price or cost", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied, or a non-Admin changing the cost", body = ErrorResponse),
        (status = 404, description = "Batch not found", body = ErrorResponse),
    )
)]
pub(super) async fn update_stock_batch(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<UpdateStockBatchRequest>,
) -> AppResult<Json<ApiResponse<StockBatch>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let batch =
        service::stock_batch::update_batch(&state.db, &key, body, user.role == Some(Role::Admin))
            .await?;

    Ok(Json(ApiResponse::success(
        batch_for(&user, batch),
        "Stock batch updated successfully",
    )))
}
