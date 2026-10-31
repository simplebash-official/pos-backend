use crate::app::AppState;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::attachments::{
    Attachment, AttachmentListQuery, AttachmentOwner, UploadAttachmentRequest,
};
use crate::modules::attachments::service;
use crate::modules::attachments::service::AttachmentContent;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

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

#[utoipa::path(
    get,
    path = "/",
    params(AttachmentListQuery),
    responses((status = 200, description = "The photos of one record", body = ApiResponse<Vec<Attachment>>)),
    security(("bearerAuth" = [])),
    tag = "Attachments"
)]
pub(super) async fn list_attachments(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Query(query): Query<AttachmentListQuery>,
) -> AppResult<Json<ApiResponse<Vec<Attachment>>>> {
    require_owner_access(&current_user, query.owner_kind, false)?;
    let list = service::list_for(&state.db, query.owner_kind, &query.owner_key).await?;
    Ok(Json(ApiResponse::success(
        list,
        "Photos retrieved successfully",
    )))
}

/// A photo can be seen (or added) only by someone who can see (or record) the record it is on.
pub(super) fn require_owner_access(
    user: &CurrentUser,
    kind: AttachmentOwner,
    write: bool,
) -> AppResult<()> {
    let (view, record) = match kind {
        AttachmentOwner::CashMovement => (perm::CASH_MOVEMENTS_VIEW, perm::CASH_MOVEMENTS_CREATE),
        AttachmentOwner::Expense => (perm::EXPENSES_VIEW, perm::EXPENSES_MANAGE),
        AttachmentOwner::Income => (perm::INCOME_VIEW, perm::INCOME_MANAGE),
    };
    user.require_permission(if write {
        perm::ATTACHMENTS_CREATE
    } else {
        perm::ATTACHMENTS_VIEW
    })?;
    user.require_permission(if write { record } else { view })
}

/// Routes of the attachments module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_attachments))
        .routes(routes!(get_attachment_content))
        // Writes
        .routes(routes!(upload_attachment))
}

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
    let name = current_user.resolve_name(&state.db).await?;
    let saved = service::upload(&state.db, current_user.user_id, name, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(saved, "Photo attached successfully")),
    ))
}
