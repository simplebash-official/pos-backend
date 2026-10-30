use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::print_jobs::PrintJobStats;
use crate::modules::print_jobs::service;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/stats", tag = modules::PRINT_JOBS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Print Jobs KPI cards: today's job count, today's revenue, open job count, and average job value", body = ApiResponse<PrintJobStats>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn get_print_job_stats(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<PrintJobStats>>> {
    let stats = service::get_print_job_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Print job stats retrieved successfully",
    )))
}
