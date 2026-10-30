use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{AnalyticsRangeQuery, CashierPerformanceResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/analytics/cashier-performance",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Per-cashier sales, discounts, refunds and product margin", body = ApiResponse<CashierPerformanceResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_analytics_cashier_performance(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Json<ApiResponse<CashierPerformanceResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_cashier_performance(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics cashier performance retrieved successfully",
    )))
}
