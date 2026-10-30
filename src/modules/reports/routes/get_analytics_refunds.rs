use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{AnalyticsRangeQuery, RefundAnalyticsResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/analytics/refunds",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Refund / returns analytics from non-voided credit notes", body = ApiResponse<RefundAnalyticsResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_analytics_refunds(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Json<ApiResponse<RefundAnalyticsResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_refunds(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics refunds retrieved successfully",
    )))
}
