use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{ReceivablesAgingQuery, ReceivablesAgingResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/analytics/receivables-aging",
    tag = modules::REPORTS,
    params(ReceivablesAgingQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Open credit invoices bucketed by age (current / 1-30 / 31-60 / 61-90 / 90+)", body = ApiResponse<ReceivablesAgingResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_analytics_receivables_aging(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<ReceivablesAgingQuery>,
) -> AppResult<Json<ApiResponse<ReceivablesAgingResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_analytics_receivables_aging(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Analytics receivables aging retrieved successfully",
    )))
}
