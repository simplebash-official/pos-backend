use super::require_owner_access::require_owner_access;
use crate::app::AppState;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;
use crate::modules::attachments::service;
use crate::modules::attachments::service::AttachmentContent;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};

#[utoipa::path(
    get,
    path = "/{key}/content",
    params(("key" = String, Path, description = "Attachment key")),
    responses(
        (status = 200, description = "The image bytes", content_type = "image/*", body = Vec<u8>),
        (status = 404, description = "Photo not found", body = ErrorResponse)
    ),
    security(("bearerAuth" = [])),
    tag = "Attachments"
)]
pub(super) async fn get_attachment_content(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Response> {
    let AttachmentContent {
        document: doc,
        bytes,
    } = service::get_content(&state.db, &key).await?;
    require_owner_access(&current_user, doc.owner_kind, false)?;
    Ok((
        [
            (header::CONTENT_TYPE, doc.content_type),
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable".to_string(),
            ),
        ],
        bytes,
    )
        .into_response())
}
