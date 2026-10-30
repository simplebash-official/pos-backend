use super::period::period;
use super::period_query::PeriodQuery;
use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::finance_reports::CashFlowResponse;
use crate::modules::finance_reports::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/cash-flow",
    params(PeriodQuery),
    responses(
        (status = 200, description = "Money in and out of each account", body = ApiResponse<CashFlowResponse>),
        (status = 400, description = "Missing or invalid dates", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Finance Reports"
)]
pub(super) async fn cash_flow(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<PeriodQuery>,
) -> AppResult<Json<ApiResponse<CashFlowResponse>>> {
    current_user.require_permission(perm::REPORTS_VIEW)?;
    let (from, to) = period(query)?;
    let report = service::cash_flow(&state.db, &from, &to).await?;
    Ok(Json(ApiResponse::success(
        report,
        "Cash flow retrieved successfully",
    )))
}
