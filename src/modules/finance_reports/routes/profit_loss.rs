use super::period::period;
use super::period_query::PeriodQuery;
use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::finance_reports::ProfitLossResponse;
use crate::modules::finance_reports::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/profit-loss",
    params(PeriodQuery),
    responses(
        (status = 200, description = "Profit and loss for the period", body = ApiResponse<ProfitLossResponse>),
        (status = 400, description = "Missing or invalid dates", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Finance Reports"
)]
pub(super) async fn profit_loss(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<PeriodQuery>,
) -> AppResult<Json<ApiResponse<ProfitLossResponse>>> {
    current_user.require_permission(perm::REPORTS_VIEW)?;
    let (from, to) = period(query)?;
    let report = service::profit_loss(&state.db, &from, &to).await?;
    Ok(Json(ApiResponse::success(
        report,
        "Profit and loss retrieved successfully",
    )))
}
