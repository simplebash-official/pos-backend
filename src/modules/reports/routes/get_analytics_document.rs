use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;

use crate::domain::reports::AnalyticsRangeQuery;
use crate::modules::documents;
use crate::modules::reports::service;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

#[utoipa::path(
    get,
    path = "/analytics/document",
    tag = modules::REPORTS,
    params(AnalyticsRangeQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Rendered analytics PDF report", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn get_analytics_document(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<AnalyticsRangeQuery>,
) -> AppResult<Response> {
    user.require_permission(perm::REPORTS_VIEW)?;
    // PDF only — CSV export is done entirely client-side from the JSON
    // endpoints, so there is no `format` param here.
    let payload = service::build_analytics_report_data(&state.db, query).await?;
    // Rendered fresh every time (no `get_or_render` cache): a report over a
    // still-open period changes with every sale and has no owning entity.
    let pdf_bytes =
        documents::service::render_uncached(&state.document_server, "analytics-report", payload)
            .await?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (
                header::CONTENT_DISPOSITION,
                "inline; filename=\"analytics-report.pdf\"",
            ),
        ],
        Body::from(pdf_bytes),
    )
        .into_response())
}
