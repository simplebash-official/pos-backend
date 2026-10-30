use super::pdf_response::pdf_response;
use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;
use crate::domain::labels::PreviewLabelsRequest;

use crate::modules::labels::service;

use axum::Json;

use axum::extract::State;

use axum::response::Response;

#[utoipa::path(post, path = "/preview", tag = modules::LABELS, request_body = PreviewLabelsRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "A one-page PDF of the unsaved design drawn with the sample", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "The design is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 422, description = "LABEL_BARCODE_TOO_NARROW or LABEL_QR_TOO_SMALL", body = ErrorResponse),
    )
)]
pub(super) async fn preview_label(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<PreviewLabelsRequest>,
) -> AppResult<Response> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let pdf = service::preview_label(
        &state.db,
        &state.document_server,
        user.tenant_id.as_deref(),
        body,
    )
    .await?;
    Ok(pdf_response(pdf))
}
