use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::SubcategoriesResponse;

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/categories/{categoryKey}/subcategories", tag = modules::INVENTORY,
    params(("categoryKey" = String, Path, description = "Category key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Subcategories for a category", body = ApiResponse<SubcategoriesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Category not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_category_subcategories(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(category_key): Path<String>,
) -> AppResult<Json<ApiResponse<SubcategoriesResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let response = service::category::get_category_subcategories(&state.db, category_key).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Subcategories retrieved successfully",
    )))
}
