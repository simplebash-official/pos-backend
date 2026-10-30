use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::cash_categories::{CashCategory, UpdateCashCategoryRequest};
use crate::modules::cash_categories::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(
    patch,
    path = "/{key}",
    params(("key" = String, Path, description = "Category key")),
    request_body = UpdateCashCategoryRequest,
    responses(
        (status = 200, description = "Category updated", body = ApiResponse<CashCategory>),
        (status = 404, description = "Category not found", body = ErrorResponse),
        (status = 409, description = "Name already used", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Categories"
)]
pub(super) async fn update_category(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<UpdateCashCategoryRequest>,
) -> AppResult<Json<ApiResponse<CashCategory>>> {
    current_user.require_permission(perm::CASH_CATEGORIES_MANAGE)?;
    let category = service::update_category(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(
        category,
        "Category updated successfully",
    )))
}
