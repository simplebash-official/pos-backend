use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::print_jobs::PrintJob;
use crate::modules::print_jobs::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/{id}", tag = modules::PRINT_JOBS,
    params(("id" = String, Path, description = "Print job ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a print job", body = ApiResponse<PrintJob>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Print job not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_print_job(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<PrintJob>>> {
    let print_job = service::get_print_job(&state.db, &id).await?;
    Ok(Json(ApiResponse::success(
        print_job,
        "Print job retrieved successfully",
    )))
}
