use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::settings::{DocumentTemplatesResponse, UpdateDocumentTemplatesRequest};
use crate::modules::settings::service;
use axum::Json;

use axum::extract::State;

/// Choose the shop's designs and set their option values. Only the kinds and
/// designs in the request change; `null` returns a kind to the standard design.
#[utoipa::path(
    put,
    path = "/document-templates",
    tag = modules::SETTINGS,
    request_body = UpdateDocumentTemplatesRequest,
    responses(
        (status = 200, description = "Document designs updated successfully", body = ApiResponse<DocumentTemplatesResponse>),
        (status = 400, description = "Unknown document kind", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin role required", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: that design does not exist or is not for this kind of document; TEMPLATE_OPTIONS_INVALID: an unknown option or a value its control does not accept", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn update_document_templates(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateDocumentTemplatesRequest>,
) -> AppResult<Json<ApiResponse<DocumentTemplatesResponse>>> {
    let response =
        service::update_document_templates(&state.db, &state.document_server, body).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Document designs updated successfully",
    )))
}
