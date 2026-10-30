use super::cache_key_for::cache_key_for;
use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;

use crate::domain::settings::DocumentDesignChoice;
use crate::modules::billing::service::print_payload;
use crate::modules::documents;
use crate::modules::quotations::service;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct DocumentQueryParams {
    /// A specific design family for this one print. Omitted → the shop's
    /// chosen design for the quotation kind.
    template: Option<String>,
    /// `original` serves the PDF exactly as first printed; `current` prints
    /// with the design in force now. Required: the caller decides.
    design: DocumentDesignChoice,
}

#[utoipa::path(get, path = "/{id}/document", tag = modules::QUOTATIONS,
    params(
        ("id" = String, Path, description = "Quotation ID or prefixed key"),
        ("template" = Option<String>, Query, description = "A design family for this print only; defaults to the shop's choice for quotations"),
        ("design" = DocumentDesignChoice, Query, description = "'original' = the stored PDF as first printed, 'current' = the design in force now"),
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Rendered (or cached) PDF", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "`design` missing", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Quotation not found, or NO_ORIGINAL_DOCUMENT: nothing was printed before", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: the requested or chosen design is not available; TEMPLATE_OPTIONS_INVALID: saved design settings no longer fit", body = ErrorResponse),
    )
)]
pub(super) async fn get_quotation_document(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<DocumentQueryParams>,
) -> AppResult<Response> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let (quotation, shop) = service::get_quotation_for_print(&state.db, &id).await?;
    let data = print_payload::build_quotation_data(&quotation, &shop);

    let pdf_bytes = documents::service::print(
        &state.db,
        &state.document_server,
        &state.config.generated_documents_dir,
        "quotation",
        params.template.as_deref(),
        params.design,
        &quotation.key,
        cache_key_for(&quotation),
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
