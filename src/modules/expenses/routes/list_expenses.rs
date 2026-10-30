use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::expenses::{ExpenseListQuery, ExpenseListResponse};
use crate::modules::expenses::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/",
    params(ExpenseListQuery),
    responses((status = 200, description = "Ledger expenses and drawer payments, newest first", body = ApiResponse<ExpenseListResponse>)),
    security(("bearerAuth" = [])),
    tag = "Expenses"
)]
pub(super) async fn list_expenses(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<ExpenseListQuery>,
) -> AppResult<Json<ApiResponse<ExpenseListResponse>>> {
    current_user.require_permission(perm::EXPENSES_VIEW)?;
    let response = service::list_expenses(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Expenses retrieved successfully",
    )))
}
