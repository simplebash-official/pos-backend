use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::cash_categories::{CashCategory, SimilarCashCategoryQuery};
use crate::modules::cash_categories::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/similar",
    params(SimilarCashCategoryQuery),
    responses((status = 200, description = "Existing categories with a similar name", body = ApiResponse<Vec<CashCategory>>)),
    security(("bearerAuth" = [])),
    tag = "Cash Categories"
)]
pub(super) async fn similar_categories(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<SimilarCashCategoryQuery>,
) -> AppResult<Json<ApiResponse<Vec<CashCategory>>>> {
    current_user.require_permission(perm::CASH_CATEGORIES_CREATE)?;
    let categories = service::similar_categories(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        categories,
        "Similar categories retrieved successfully",
    )))
}
