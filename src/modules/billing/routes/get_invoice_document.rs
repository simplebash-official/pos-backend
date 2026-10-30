use super::document_query_params::DocumentQueryParams;

use super::invoice_variant::invoice_variant;

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

#[utoipa::path(get, path = "/invoices/{id}/documents/{documentType}", tag = modules::BILLING,
    params(
        ("id" = String, Path, description = "Invoice ID or prefixed key"),
        ("documentType" = String, Path, description = "'a4-invoice' or 'thermal-receipt'"),
        ("paperWidthMm" = Option<u32>, Query, description = "58 or 80 — thermal-receipt only, defaults to 80 (D9)"),
        ("template" = Option<String>, Query, description = "A design family for this print only; defaults to the shop's choice for the document kind"),
        ("design" = DocumentDesignChoice, Query, description = "'original' = the stored PDF as first printed, 'current' = the design in force now"),
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Rendered (or cached) PDF", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Unknown documentType, or `design` missing", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Invoice not found, or NO_ORIGINAL_DOCUMENT: nothing was printed before", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: the requested or chosen design is not available; TEMPLATE_OPTIONS_INVALID: saved design settings no longer fit", body = ErrorResponse),
    )
)]
pub(super) async fn get_invoice_document(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path((id, document_type)): Path<(String, String)>,
    Query(params): Query<DocumentQueryParams>,
) -> AppResult<Response> {
    let (kind, legacy_cache_key) = invoice_variant(&document_type, params.paper_width_mm)?;
    let invoice = service::get_invoice(&state.db, &id).await?;
    // Sum of every payment against this invoice (checkout deposit + any
    // installments) so a partial-credit slip prints the right "Amount Paid"
    // / "Balance Due".
    let paid_so_far_cents: i64 = service::payments::list_payments(&state.db, &invoice.key)
        .await?
        .iter()
        .map(|p| p.amount_cents)
        .sum();

    let data = match kind {
        "a4-invoice" => service::print_payload::build_a4_invoice_data(
            &invoice,
            "ORIGINAL — CUSTOMER COPY",
            false,
            paid_so_far_cents,
        ),
        _ => service::print_payload::build_thermal_receipt_data(
            &invoice,
            params.paper_width_mm.unwrap_or(80),
            paid_so_far_cents,
        ),
    };

    let pdf_bytes = documents::service::print(
        &state.db,
        &state.document_server,
        &state.config.generated_documents_dir,
        kind,
        params.template.as_deref(),
        params.design,
        &invoice.key,
        &legacy_cache_key,
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
