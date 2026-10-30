use super::require_owner_access::require_owner_access;
use crate::app::AppState;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::attachments::{Attachment, UploadAttachmentRequest};
use crate::modules::attachments::service;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(
    post,
    path = "/",
    request_body = UploadAttachmentRequest,
    responses(
        (status = 201, description = "Photo attached", body = ApiResponse<Attachment>),
        (status = 400, description = "Not a usable photo", body = ErrorResponse),
        (status = 409, description = "The record already holds the most photos", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Attachments"
)]
pub(super) async fn upload_attachment(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Json(body): Json<UploadAttachmentRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Attachment>>)> {
    require_owner_access(&current_user, body.owner_kind, true)?;
    let name = current_user.resolve_name(&state.db).await;
    let saved = service::upload(&state.db, current_user.user_id, name, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(saved, "Photo attached successfully")),
    ))
}
