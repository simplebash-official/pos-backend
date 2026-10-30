use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::cash_categories::{CashCategory, CashCategoryListQuery};
use crate::modules::cash_categories::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/",
    params(CashCategoryListQuery),
    responses((status = 200, description = "Categories, best first", body = ApiResponse<Vec<CashCategory>>)),
    security(("bearerAuth" = [])),
    tag = "Cash Categories"
)]
pub(super) async fn list_categories(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<CashCategoryListQuery>,
) -> AppResult<Json<ApiResponse<Vec<CashCategory>>>> {
    current_user.require_permission(perm::CASH_MOVEMENTS_VIEW)?;
    // Seeing hidden categories or the review queue is part of managing them.
    if query.include_hidden == Some(true) || query.needs_review == Some(true) {
        current_user.require_permission(perm::CASH_CATEGORIES_MANAGE)?;
    }
    let categories = service::list_categories(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        categories,
        "Categories retrieved successfully",
    )))
}
