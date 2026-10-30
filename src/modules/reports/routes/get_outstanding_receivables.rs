use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{OutstandingReceivablesQuery, OutstandingReceivablesResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/outstanding",
    tag = modules::REPORTS,
    params(OutstandingReceivablesQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Outstanding credit receivables report with overdue aging", body = ApiResponse<OutstandingReceivablesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_outstanding_receivables(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<OutstandingReceivablesQuery>,
) -> AppResult<Json<ApiResponse<OutstandingReceivablesResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_outstanding_report(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Outstanding receivables report retrieved successfully",
    )))
}
