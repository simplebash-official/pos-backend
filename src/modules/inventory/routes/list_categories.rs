use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::CategoriesResponse;

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/categories", tag = modules::INVENTORY,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List categories with subcategories", body = ApiResponse<CategoriesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn list_categories(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<CategoriesResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let response = service::category::list_categories(&state.db).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Categories retrieved successfully",
    )))
}
