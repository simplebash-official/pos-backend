use super::period::period;
use super::period_query::PeriodQuery;
use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::finance_reports::ExpenseBreakdownResponse;
use crate::modules::finance_reports::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/expenses",
    params(PeriodQuery),
    responses(
        (status = 200, description = "Costs per category and month", body = ApiResponse<ExpenseBreakdownResponse>),
        (status = 400, description = "Missing or invalid dates", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Finance Reports"
)]
pub(super) async fn expense_breakdown(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<PeriodQuery>,
) -> AppResult<Json<ApiResponse<ExpenseBreakdownResponse>>> {
    current_user.require_permission(perm::REPORTS_VIEW)?;
    let (from, to) = period(query)?;
    let report = service::expense_breakdown(&state.db, &from, &to).await?;
    Ok(Json(ApiResponse::success(
        report,
        "Expense breakdown retrieved successfully",
    )))
}
