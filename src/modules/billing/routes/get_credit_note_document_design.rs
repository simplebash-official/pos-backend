use super::design_status_params::DesignStatusParams;

use super::require_credit_note_type::require_credit_note_type;

use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::settings::DocumentDesignStatus;
use crate::modules::billing::service;

use crate::modules::documents;
use axum::Json;

use axum::extract::{Path, Query, State};

#[utoipa::path(get, path = "/credit-notes/{id}/documents/{documentType}/design", tag = modules::BILLING,
    params(
        ("id" = String, Path, description = "Credit note ID or prefixed key"),
        ("documentType" = String, Path, description = "'credit-note' — the only supported value"),
        ("template" = Option<String>, Query, description = "A design family for this print only"),
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What the stored PDF was printed with versus what would print now", body = ApiResponse<DocumentDesignStatus>),
        (status = 400, description = "Unknown documentType", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Credit note not found", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: the requested or chosen design is not available", body = ErrorResponse),
    )
)]
pub(super) async fn get_credit_note_document_design(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path((id, document_type)): Path<(String, String)>,
    Query(params): Query<DesignStatusParams>,
) -> AppResult<Json<ApiResponse<DocumentDesignStatus>>> {
    require_credit_note_type(&document_type)?;
    let credit_note = service::credit_notes::get_credit_note(&state.db, &id).await?;
    let status = documents::service::print_design_status(
        &state.db,
        &state.document_server,
        "credit-note",
        params.template.as_deref(),
        &credit_note.key,
        "credit-note",
    )
    .await?;
    Ok(Json(ApiResponse::success(
        status,
        "Document design status retrieved successfully",
    )))
}
