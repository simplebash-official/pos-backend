// HTTP layer only — mirrors `modules::repairs::routes` exactly.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::{modules, permissions as perm},
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::print_jobs::{
        CreatePrintJobRequest, PrintJob, PrintJobListQuery, PrintJobListResponse, PrintJobStats,
        UpdatePrintJobRequest,
    },
    modules::print_jobs::service,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_print_jobs, create_print_job))
        .routes(routes!(get_print_job_stats))
        .routes(routes!(get_print_job, update_print_job, delete_print_job))
}

#[utoipa::path(get, path = "/", tag = modules::PRINT_JOBS, params(PrintJobListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List print jobs with search, status filtering, and pagination", body = ApiResponse<PrintJobListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
async fn list_print_jobs(
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

#[utoipa::path(get, path = "/stats", tag = modules::PRINT_JOBS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Print Jobs KPI cards: today's job count, today's revenue, open job count, and average job value", body = ApiResponse<PrintJobStats>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
async fn get_print_job_stats(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<PrintJobStats>>> {
    let stats = service::get_print_job_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Print job stats retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/{id}", tag = modules::PRINT_JOBS,
    params(("id" = String, Path, description = "Print job ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a print job", body = ApiResponse<PrintJob>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Print job not found", body = ErrorResponse),
    )
)]
async fn get_print_job(
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

#[utoipa::path(post, path = "/", tag = modules::PRINT_JOBS, request_body = CreatePrintJobRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Print job created", body = ApiResponse<PrintJob>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn create_print_job(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Json(body): Json<CreatePrintJobRequest>,
) -> AppResult<Json<ApiResponse<PrintJob>>> {
    user.require_permission(perm::PRINT_JOBS_WRITE)?;
    let print_job = service::create_print_job(&state.db, body, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        print_job,
        "Print job created successfully",
    )))
}

#[utoipa::path(patch, path = "/{id}", tag = modules::PRINT_JOBS,
    params(("id" = String, Path, description = "Print job ID or prefixed key")),
    request_body = UpdatePrintJobRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Print job updated", body = ApiResponse<PrintJob>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Print job not found", body = ErrorResponse),
    )
)]
async fn update_print_job(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
    Json(body): Json<UpdatePrintJobRequest>,
) -> AppResult<Json<ApiResponse<PrintJob>>> {
    user.require_permission(perm::PRINT_JOBS_WRITE)?;
    let print_job = service::update_print_job(&state.db, &id, body, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        print_job,
        "Print job updated successfully",
    )))
}

#[utoipa::path(delete, path = "/{id}", tag = modules::PRINT_JOBS,
    params(("id" = String, Path, description = "Print job ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Print job deleted", body = ApiResponse<PrintJob>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Print job not found", body = ErrorResponse),
    )
)]
async fn delete_print_job(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<PrintJob>>> {
    user.require_permission(perm::PRINT_JOBS_WRITE)?;
    let print_job = service::delete_print_job(&state.db, &id, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        print_job,
        "Print job deleted successfully",
    )))
}
