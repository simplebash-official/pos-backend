use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::expenses::ExpenseStats;
use crate::modules::expenses::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(
    get,
    path = "/stats",
    responses((status = 200, description = "This month's costs and what is still owed", body = ApiResponse<ExpenseStats>)),
    security(("bearerAuth" = [])),
    tag = "Expenses"
)]
pub(super) async fn get_expense_stats(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<ExpenseStats>>> {
    current_user.require_permission(perm::EXPENSES_VIEW)?;
    let stats = service::get_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Expense stats retrieved successfully",
    )))
}
