use super::require_owner_access::require_owner_access;
use crate::app::AppState;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::attachments::{Attachment, AttachmentListQuery};
use crate::modules::attachments::service;
use axum::Json;
use axum::extract::{Query, State};

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
