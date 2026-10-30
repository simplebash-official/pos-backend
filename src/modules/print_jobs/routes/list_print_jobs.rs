use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::print_jobs::{PrintJobListQuery, PrintJobListResponse};
use crate::modules::print_jobs::service;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/", tag = modules::PRINT_JOBS, params(PrintJobListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List print jobs with search, status filtering, and pagination", body = ApiResponse<PrintJobListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn list_print_jobs(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<PrintJobListQuery>,
) -> AppResult<Json<ApiResponse<PrintJobListResponse>>> {
    let response = service::list_print_jobs(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Print jobs retrieved successfully",
    )))
}
