use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{CategoryInfo, UpdateCategoryRequest};

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(put, path = "/categories/{categoryKey}", tag = modules::INVENTORY,
    params(("categoryKey" = String, Path, description = "Category key")),
    request_body = UpdateCategoryRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Category updated", body = ApiResponse<CategoryInfo>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Category not found", body = ErrorResponse),
        (status = 409, description = "New name already in use", body = ErrorResponse),
    )
)]
pub(super) async fn update_category(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(category_key): Path<String>,
    Json(body): Json<UpdateCategoryRequest>,
) -> AppResult<Json<ApiResponse<CategoryInfo>>> {
    let updated = service::category::update_category(&state.db, category_key, body).await?;

    Ok(Json(ApiResponse::success(
        updated,
        "Category updated successfully",
    )))
}
