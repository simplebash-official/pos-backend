use super::pdf_response::pdf_response;
use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;
use crate::domain::labels::RenderLabelsRequest;

use crate::modules::labels::service;

use axum::Json;

use axum::extract::State;

use axum::response::Response;

#[utoipa::path(post, path = "/render", tag = modules::LABELS, request_body = RenderLabelsRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "A PDF with one page per sticker", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Nothing to print, or too many labels", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Design, product or serial number not found", body = ErrorResponse),
        (status = 409, description = "SERIAL_ALREADY_SOLD: the unit is not in stock", body = ErrorResponse),
        (status = 422, description = "LABEL_PRODUCT_WITHOUT_BARCODE, LABEL_PRICE_REQUIRED, PRICE_TIER_UNAVAILABLE, LABEL_BARCODE_TOO_NARROW or LABEL_QR_TOO_SMALL", body = ErrorResponse),
    )
)]
/// The print window's preview: renders without recording anything. Real
/// prints go through `POST /labels/jobs`. Admin and Manager only.
pub(super) async fn render_labels(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<RenderLabelsRequest>,
) -> AppResult<Response> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let pdf = service::render_labels(
        &state.db,
        &state.document_server,
        user.tenant_id.as_deref(),
        body,
    )
    .await?;
    Ok(pdf_response(pdf))
}
