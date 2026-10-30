use super::document_query_params::DocumentQueryParams;

use super::require_credit_note_type::require_credit_note_type;

use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;

use crate::domain::settings::DocumentDesignChoice;
use crate::modules::billing::service;

use crate::modules::documents;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

#[utoipa::path(get, path = "/credit-notes/{id}/documents/{documentType}", tag = modules::BILLING,
    params(
        ("id" = String, Path, description = "Credit note ID or prefixed key"),
        ("documentType" = String, Path, description = "'credit-note' — the only supported value"),
        ("template" = Option<String>, Query, description = "A design family for this print only; defaults to the shop's choice"),
        ("design" = DocumentDesignChoice, Query, description = "'original' = the stored PDF as first printed, 'current' = the design in force now"),
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Rendered (or cached) PDF", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Unknown documentType, or `design` missing", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Credit note not found, or NO_ORIGINAL_DOCUMENT: nothing was printed before", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: the requested or chosen design is not available; TEMPLATE_OPTIONS_INVALID: saved design settings no longer fit", body = ErrorResponse),
    )
)]
pub(super) async fn get_credit_note_document(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path((id, document_type)): Path<(String, String)>,
    Query(params): Query<DocumentQueryParams>,
) -> AppResult<Response> {
    require_credit_note_type(&document_type)?;

    let credit_note = service::credit_notes::get_credit_note(&state.db, &id).await?;
    let invoice = match &credit_note.invoice_key {
        Some(invoice_key) => Some(service::get_invoice(&state.db, invoice_key).await?),
        None => None,
    };
    let data = service::print_payload::build_credit_note_data(&credit_note, invoice.as_ref());

    let pdf_bytes = documents::service::print(
        &state.db,
        &state.document_server,
        &state.config.generated_documents_dir,
        "credit-note",
        params.template.as_deref(),
        params.design,
        &credit_note.key,
        "credit-note",
        data,
    )
    .await?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/pdf")],
        Body::from(pdf_bytes),
    )
        .into_response())
}
