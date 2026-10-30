use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::labels::LabelTemplatesResponse;

use crate::modules::labels::service;

use axum::Json;

use axum::extract::State;

#[utoipa::path(get, path = "/templates", tag = modules::LABELS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The ready-made designs, then the shop's own", body = ApiResponse<LabelTemplatesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn list_templates(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<LabelTemplatesResponse>>> {
    user.require_permission(perm::INVENTORY_READ)?;
    let templates = service::list_templates(&state.db).await?;
    Ok(Json(ApiResponse::success(
        templates,
        "Label designs retrieved successfully",
    )))
}
