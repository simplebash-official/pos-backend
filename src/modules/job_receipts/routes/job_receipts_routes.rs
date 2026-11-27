//! `GET /api/job-receipts/document`: the slip for one or more jobs of one customer.

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;
use crate::domain::job_receipts::{JobReceiptKey, JobReceiptKind, JobReceiptQuery};
use crate::modules::job_receipts::service;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(get, path = "/document", tag = modules::JOB_RECEIPTS,
    params(JobReceiptQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The slip as the jobs stand now", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "`paper` missing or unknown; JOB_RECEIPT_JOBS_INVALID: not 1 to 20 distinct repair or print job keys; JOB_RECEIPT_CUSTOMERS_DIFFER: the jobs are not one customer's", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "repairs:write is needed for a repair on the slip, print_jobs:write for a print job", body = ErrorResponse),
        (status = 404, description = "A job on the slip was not found", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: the requested or chosen design is not available; TEMPLATE_OPTIONS_INVALID: saved design settings no longer fit", body = ErrorResponse),
    )
)]
/// Prints one slip for the jobs asked for, with the design in force now.
pub(super) async fn get_job_receipt(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<JobReceiptQuery>,
) -> AppResult<Response> {
    let keys = JobReceiptKey::parse_list(&query.jobs)?;
    // Whoever can receive a job can print its slip; checked before any ticket is read.
    for key in &keys {
        user.require_permission(match key.kind {
            JobReceiptKind::Repair => perm::REPAIRS_WRITE,
            JobReceiptKind::Print => perm::PRINT_JOBS_WRITE,
        })?;
    }
    let receipt = service::load(&state.db, &keys).await?;
    let pdf_bytes =
        service::print(&state, &receipt, query.paper, query.template.as_deref()).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(pdf_bytes),
    )
        .into_response())
}

/// The routes of job slips.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_job_receipt))
}
