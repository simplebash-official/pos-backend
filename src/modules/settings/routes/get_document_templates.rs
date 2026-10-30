use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::settings::DocumentTemplatesResponse;
use crate::modules::settings::service;
use axum::Json;

use axum::extract::State;

/// The shop's document designs: for each kind (A4 invoice, receipt, credit
/// note) the designs available and which one is in use.
#[utoipa::path(
    get,
    path = "/document-templates",
    tag = modules::SETTINGS,
    responses(
        (status = 200, description = "Document designs retrieved successfully", body = ApiResponse<DocumentTemplatesResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_document_templates(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<DocumentTemplatesResponse>>> {
    let response = service::get_document_templates(&state.db, &state.document_server).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Document designs retrieved successfully",
    )))
}
