use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::incomes::{IncomeListQuery, IncomeListResponse};
use crate::modules::incomes::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/",
    params(IncomeListQuery),
    responses((status = 200, description = "Ledger income and drawer additions, newest first", body = ApiResponse<IncomeListResponse>)),
    security(("bearerAuth" = [])),
    tag = "Income"
)]
pub(super) async fn list_incomes(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<IncomeListQuery>,
) -> AppResult<Json<ApiResponse<IncomeListResponse>>> {
    current_user.require_permission(perm::INCOME_VIEW)?;
    let response = service::list_incomes(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Income retrieved successfully",
    )))
}
