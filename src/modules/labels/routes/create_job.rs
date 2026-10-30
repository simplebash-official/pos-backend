use super::label_job_key_header::LABEL_JOB_KEY_HEADER;
use super::pdf_response::pdf_response;
use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;
use crate::domain::labels::CreateLabelJobRequest;

use crate::modules::labels::service;
use crate::modules::users;
use axum::Json;

use axum::extract::State;
use axum::http::HeaderValue;
use axum::response::Response;

/// The printing person's name for the history: the token's own name (identity
/// tokens carry one), else the account's, else "Unknown" (same as billing's
/// recorded-by snapshot).
async fn printed_by_name(state: &AppState, user: &CurrentUser) -> String {
    if let Some(name) = user.name.as_deref().filter(|n| !n.trim().is_empty()) {
        return name.to_string();
    }
    match mongodb::bson::oid::ObjectId::parse_str(&user.user_id) {
        Ok(id) => users::service::get_user(&state.db, id)
            .await
            .map(|u| u.name)
            .unwrap_or_else(|_| "Unknown".to_string()),
        Err(_) => "Unknown".to_string(),
    }
}

#[utoipa::path(post, path = "/jobs", tag = modules::LABELS, request_body = CreateLabelJobRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The PDF to print or save (one page per sticker); the recorded job's key is in the X-Label-Job-Key header", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Nothing to print, too many labels, or the print method is incomplete", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Design, product or serial number not found", body = ErrorResponse),
        (status = 409, description = "SERIAL_ALREADY_SOLD: the unit is not in stock", body = ErrorResponse),
        (status = 422, description = "LABEL_PRODUCT_WITHOUT_BARCODE, LABEL_PRICE_REQUIRED, PRICE_TIER_UNAVAILABLE, LABEL_BARCODE_TOO_NARROW or LABEL_QR_TOO_SMALL", body = ErrorResponse),
    )
)]
pub(super) async fn create_job(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateLabelJobRequest>,
) -> AppResult<Response> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let name = printed_by_name(&state, &user).await;
    let (job, pdf) = service::jobs::create_job(
        &state.db,
        &state.document_server,
        user.tenant_id.as_deref(),
        &user.user_id,
        &name,
        body,
    )
    .await?;
    let mut response = pdf_response(pdf);
    let key = HeaderValue::from_str(&job.key)
        .map_err(|e| crate::core::error::AppError::internal(format!("Bad job key: {e}")))?;
    response.headers_mut().insert(LABEL_JOB_KEY_HEADER, key);
    Ok(response)
}
