use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::labels::{LabelTemplate, LabelTemplateInput};

use crate::modules::labels::service;

use axum::Json;

use axum::extract::{Path, State};

#[utoipa::path(put, path = "/templates/{key}", tag = modules::LABELS, request_body = LabelTemplateInput,
    params(("key" = String, Path, description = "Design key (ltpl_...)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Design saved", body = ApiResponse<LabelTemplate>),
        (status = 400, description = "The design is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "LABEL_TEMPLATE_NOT_FOUND", body = ErrorResponse),
        (status = 409, description = "LABEL_TEMPLATE_BUILT_IN or LABEL_TEMPLATE_NAME_TAKEN", body = ErrorResponse),
    )
)]
pub(super) async fn update_template(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<LabelTemplateInput>,
) -> AppResult<Json<ApiResponse<LabelTemplate>>> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let template = service::update_template(&state.db, &key, body).await?;
    Ok(Json(ApiResponse::success(
        template,
        "Label design saved successfully",
    )))
}
