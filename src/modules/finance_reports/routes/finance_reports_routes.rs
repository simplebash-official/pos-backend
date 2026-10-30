use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::finance_reports::{
    CashFlowResponse, ExpenseBreakdownResponse, PayablesResponse, ProfitLossResponse,
};
use crate::modules::finance_reports::service;
use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

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

#[utoipa::path(
    get,
    path = "/payables",
    responses((status = 200, description = "Bills and supplier balances still to pay", body = ApiResponse<PayablesResponse>)),
    security(("bearerAuth" = [])),
    tag = "Finance Reports"
)]
pub(super) async fn payables(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> AppResult<Json<ApiResponse<PayablesResponse>>> {
    current_user.require_permission(perm::REPORTS_VIEW)?;
    let report = service::payables(&state.db).await?;
    Ok(Json(ApiResponse::success(
        report,
        "Payables retrieved successfully",
    )))
}

pub(super) fn period(query: PeriodQuery) -> AppResult<(String, String)> {
    match (query.from, query.to) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(AppError::validation(
            "Please pick the first and the last day of the report.",
        )),
    }
}

/// The days a report covers, both inclusive.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub(super) struct PeriodQuery {
    /// First day, `YYYY-MM-DD`.
    pub(super) from: Option<String>,
    /// Last day, `YYYY-MM-DD`.
    pub(super) to: Option<String>,
}

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

/// Routes of the finance reports module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Profit and costs
        .routes(routes!(profit_loss))
        .routes(routes!(expense_breakdown))
        // Money
        .routes(routes!(cash_flow))
        .routes(routes!(payables))
}
