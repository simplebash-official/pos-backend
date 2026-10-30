use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::labels::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(delete, path = "/templates/{key}", tag = modules::LABELS,
    params(("key" = String, Path, description = "Design key (ltpl_...)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Design deleted"),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "LABEL_TEMPLATE_NOT_FOUND", body = ErrorResponse),
        (status = 409, description = "LABEL_TEMPLATE_BUILT_IN: ready-made designs cannot be deleted", body = ErrorResponse),
    )
)]
pub(super) async fn delete_template(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<()>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    service::delete_template(&state.db, &key).await?;
    Ok(Json(ApiResponse::message(
        "Label design deleted successfully",
    )))
}
