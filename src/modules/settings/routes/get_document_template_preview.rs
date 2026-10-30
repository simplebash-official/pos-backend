use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;

use crate::modules::settings::service;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

/// A PDF of one design rendered with its own sample data.
#[utoipa::path(
    get,
    path = "/document-templates/{family}/preview",
    tag = modules::SETTINGS,
    params(("family" = String, Path, description = "The design family, as listed by GET /settings/document-templates")),
    responses(
        (status = 200, description = "Sample PDF", content_type = "application/pdf", body = Vec<u8>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "TEMPLATE_UNAVAILABLE: no such design, or it has no sample", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_document_template_preview(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(family): Path<String>,
) -> AppResult<Response> {
    let pdf = service::render_document_template_preview(&state.document_server, &family).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(pdf),
    )
        .into_response())
}
