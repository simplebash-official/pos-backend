use super::design_status_params::DesignStatusParams;

use super::invoice_variant::invoice_variant;

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

#[utoipa::path(get, path = "/invoices/{id}/documents/{documentType}/design", tag = modules::BILLING,
    params(
        ("id" = String, Path, description = "Invoice ID or prefixed key"),
        ("documentType" = String, Path, description = "'a4-invoice' or 'thermal-receipt'"),
        ("paperWidthMm" = Option<u32>, Query, description = "58 or 80 — thermal-receipt only, defaults to 80"),
        ("template" = Option<String>, Query, description = "A design family for this print only"),
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What the stored PDF was printed with versus what would print now", body = ApiResponse<DocumentDesignStatus>),
        (status = 400, description = "Unknown documentType", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: the requested or chosen design is not available", body = ErrorResponse),
    )
)]
pub(super) async fn get_invoice_document_design(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path((id, document_type)): Path<(String, String)>,
    Query(params): Query<DesignStatusParams>,
) -> AppResult<Json<ApiResponse<DocumentDesignStatus>>> {
    let (kind, legacy_cache_key) = invoice_variant(&document_type, params.paper_width_mm)?;
    let invoice = service::get_invoice(&state.db, &id).await?;
    let status = documents::service::print_design_status(
        &state.db,
        &state.document_server,
        kind,
        params.template.as_deref(),
        &invoice.key,
        &legacy_cache_key,
    )
    .await?;
    Ok(Json(ApiResponse::success(
        status,
        "Document design status retrieved successfully",
    )))
}
