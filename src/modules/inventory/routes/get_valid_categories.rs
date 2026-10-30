use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::ValidCategoriesResponse;

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/categories/valid", tag = modules::INVENTORY,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Valid category/subcategory validation metadata", body = ApiResponse<ValidCategoriesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_valid_categories(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<ValidCategoriesResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let response = service::category::get_valid_categories(&state.db).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Valid categories retrieved successfully",
    )))
}
