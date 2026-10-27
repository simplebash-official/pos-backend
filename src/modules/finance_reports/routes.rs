// HTTP layer for the money reports. Read-only; every route needs `reports:view` (Admin and Manager).

use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use utoipa::IntoParams;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::permissions as perm,
        error::{AppError, AppResult},
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::finance_reports::{
        CashFlowResponse, ExpenseBreakdownResponse, PayablesResponse, ProfitLossResponse,
    },
    modules::finance_reports::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Profit and costs
        .routes(routes!(profit_loss))
        .routes(routes!(expense_breakdown))
        // Money
        .routes(routes!(cash_flow))
        .routes(routes!(payables))
}

// ============================================================================
// Helpers
// ============================================================================

/// The days a report covers, both inclusive.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
struct PeriodQuery {
    /// First day, `YYYY-MM-DD`.
    from: Option<String>,
    /// Last day, `YYYY-MM-DD`.
    to: Option<String>,
}

fn period(query: PeriodQuery) -> AppResult<(String, String)> {
    match (query.from, query.to) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err(AppError::validation(
            "Please pick the first and the last day of the report.",
        )),
    }
}

// ============================================================================
// Profit and costs
// ============================================================================

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
async fn profit_loss(
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
async fn expense_breakdown(
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

// ============================================================================
// Money
// ============================================================================

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
async fn cash_flow(
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
    path = "/payables",
    responses((status = 200, description = "Bills and supplier balances still to pay", body = ApiResponse<PayablesResponse>)),
    security(("bearerAuth" = [])),
    tag = "Finance Reports"
)]
async fn payables(
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
