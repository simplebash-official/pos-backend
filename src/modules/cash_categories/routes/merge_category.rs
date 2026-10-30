use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::cash_categories::{CashCategory, MergeCashCategoryRequest};
use crate::modules::cash_categories::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(
    post,
    path = "/{key}/merge",
    params(("key" = String, Path, description = "Key of the duplicate to merge away")),
    request_body = MergeCashCategoryRequest,
    responses(
        (status = 200, description = "Merged; the surviving category", body = ApiResponse<CashCategory>),
        (status = 404, description = "Category not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Cash Categories"
)]
pub(super) async fn merge_category(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<MergeCashCategoryRequest>,
) -> AppResult<Json<ApiResponse<CashCategory>>> {
    current_user.require_permission(perm::CASH_CATEGORIES_MANAGE)?;
    let category = service::merge_category(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(
        category,
        "Categories merged successfully",
    )))
}
