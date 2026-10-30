use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{DailySalesQuery, DailySalesReportResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/daily-sales",
    tag = modules::REPORTS,
    params(DailySalesQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Daily sales report with stream and payment method breakdowns", body = ApiResponse<DailySalesReportResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_daily_sales(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<DailySalesQuery>,
) -> AppResult<Json<ApiResponse<DailySalesReportResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_daily_sales_report(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Daily sales report retrieved successfully",
    )))
}
