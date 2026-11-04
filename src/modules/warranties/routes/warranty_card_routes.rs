//! `GET /api/warranties/{key}/documents/warranty-card`: the printed warranty card.

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ErrorResponse;
use crate::domain::settings::DocumentDesignChoice;
use crate::domain::warranties::WarrantyCardQuery;
use crate::modules::billing;
use crate::modules::documents;
use crate::modules::warranties::service;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

/// The document kind a warranty card is printed as.
const KIND: &str = "warranty-card";

#[utoipa::path(get, path = "/{key}/documents/warranty-card", tag = modules::WARRANTIES,
    params(
        ("key" = String, Path, description = "Warranty key (`wty_…`)"),
        WarrantyCardQuery,
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The warranty card as it stands now", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "`paper` missing or not one of a5, thermal-58, thermal-80", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Warranty not found", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: the requested or chosen design is not available; TEMPLATE_OPTIONS_INVALID: saved design settings no longer fit", body = ErrorResponse),
    )
)]
/// Prints one warranty's card with the design in force now. A card always shows the warranty
/// as it is today, so each change to the warranty (its `version`) prints afresh; there is no
/// "original" to reprint.
pub(super) async fn get_warranty_card(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
    Query(query): Query<WarrantyCardQuery>,
) -> AppResult<Response> {
    user.require_permission(perm::WARRANTIES_VIEW)?;
    let detail = service::detail(&state.db, &key).await?;
    let warranty = &detail.warranty;
    let invoice = billing::service::get_invoice(&state.db, &warranty.invoice_key).await?;
    let replaces = detail
        .replaces
        .as_ref()
        .map(|old| old.warranty_number.as_str());
    let data = billing::service::print_payload::build_warranty_card_data(
        warranty,
        &invoice,
        query.paper,
        replaces,
    )?;
    let cache_key = format!("{KIND}-{}-v{}", query.paper.as_str(), warranty.version);
    let pdf_bytes = documents::service::print(
        &state.db,
        &state.document_server,
        &state.config.generated_documents_dir,
        KIND,
        query.template.as_deref(),
        DocumentDesignChoice::Current,
        &warranty.key,
        &cache_key,
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
