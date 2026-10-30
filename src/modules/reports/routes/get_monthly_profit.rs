use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{MonthlyProfitQuery, MonthlyProfitReportResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/monthly-profit",
    tag = modules::REPORTS,
    params(MonthlyProfitQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Monthly profit report with revenue, COGS, commissions, and refunds", body = ApiResponse<MonthlyProfitReportResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_monthly_profit(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<MonthlyProfitQuery>,
) -> AppResult<Json<ApiResponse<MonthlyProfitReportResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_monthly_profit_report(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Monthly profit report retrieved successfully",
    )))
}
