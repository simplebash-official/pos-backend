use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::labels::{LabelTemplate, LabelTemplateInput};

use crate::modules::labels::service;

use axum::Json;

use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(post, path = "/templates", tag = modules::LABELS, request_body = LabelTemplateInput,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Design saved", body = ApiResponse<LabelTemplate>),
        (status = 400, description = "The design is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 409, description = "LABEL_TEMPLATE_NAME_TAKEN: another design has this name", body = ErrorResponse),
    )
)]
pub(super) async fn create_template(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<LabelTemplateInput>,
) -> AppResult<(StatusCode, Json<ApiResponse<LabelTemplate>>)> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let template = service::create_template(&state.db, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            template,
            "Label design saved successfully",
        )),
    ))
}
