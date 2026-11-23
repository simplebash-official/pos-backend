//! `GET /api/print-jobs/{id}/documents/job-receipt`: the receipt a customer gets when the shop
//! receives a print job.

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;
use crate::domain::job_receipts::{JobReceiptQuery, JobReceiptSource};
use crate::modules::job_receipts;
use crate::modules::print_jobs::service;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(get, path = "/{id}/documents/job-receipt", tag = modules::PRINT_JOBS,
    params(
        ("id" = String, Path, description = "Print job ID or prefixed key"),
        JobReceiptQuery,
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The receipt as the job stands now", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "`paper` missing or not one of a5, thermal-58, thermal-80", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Print job not found", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: the requested or chosen design is not available; TEMPLATE_OPTIONS_INVALID: saved design settings no longer fit", body = ErrorResponse),
    )
)]
/// Prints the receipt of a print job with the design in force now.
pub(super) async fn get_print_job_receipt(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<JobReceiptQuery>,
) -> AppResult<Response> {
    user.require_permission(perm::PRINT_JOBS_WRITE)?;
    let job = service::get_print_job(&state.db, &id).await?;
    let source = JobReceiptSource::from_print_job(&job)?;
    let pdf_bytes =
        job_receipts::service::print(&state, &source, query.paper, query.template.as_deref())
            .await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(pdf_bytes),
    )
        .into_response())
}

/// The job receipt route of this module.
pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_print_job_receipt))
}
