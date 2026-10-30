use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::expenses::ExpenseDetail;
use crate::modules::expenses::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(
    get,
    path = "/{key}",
    params(("key" = String, Path, description = "Expense key")),
    responses(
        (status = 200, description = "The expense with its payments", body = ApiResponse<ExpenseDetail>),
        (status = 404, description = "Expense not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Expenses"
)]
pub(super) async fn get_expense(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<ExpenseDetail>>> {
    current_user.require_permission(perm::EXPENSES_VIEW)?;
    let detail = service::get_expense(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        detail,
        "Expense retrieved successfully",
    )))
}
