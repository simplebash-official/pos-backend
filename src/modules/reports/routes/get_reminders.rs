use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::reports::{RemindersQuery, RemindersResponse};

use crate::modules::reports::service;
use axum::Json;

use axum::extract::{Query, State};

#[utoipa::path(
    get,
    path = "/reminders",
    tag = modules::REPORTS,
    params(RemindersQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Dashboard reminders: overdue and due-soon credit invoices and repair/print jobs", body = ApiResponse<RemindersResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_reminders(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<RemindersQuery>,
) -> AppResult<Json<ApiResponse<RemindersResponse>>> {
    user.require_permission(perm::REPORTS_VIEW)?;
    let response = service::get_reminders(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Reminders retrieved successfully",
    )))
}
