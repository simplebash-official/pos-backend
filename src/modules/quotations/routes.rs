// HTTP layer for quotations. Every route requires `quotations:manage`
// (Admin and Manager only), checked as the first line of each handler — a
// Staff token gets 403 here no matter what the UI shows.

use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::{modules, permissions as perm},
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::quotations::{
        CreateQuotationRequest, Quotation, QuotationListQuery, QuotationListResponse,
        STATUS_CANCELLED,
    },
    domain::settings::{DocumentDesignChoice, DocumentDesignStatus},
    modules::{billing::service::print_payload, documents, quotations::service},
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_quotations, create_quotation))
        .routes(routes!(get_quotation))
        .routes(routes!(cancel_quotation))
        .routes(routes!(get_quotation_document))
        .routes(routes!(get_quotation_document_design))
}

// ============================================================================
// Quotations
// ============================================================================

#[utoipa::path(get, path = "/", tag = modules::QUOTATIONS, params(QuotationListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List quotations with search, status filter and pagination", body = ApiResponse<QuotationListResponse>),
        (status = 400, description = "Invalid status filter", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn list_quotations(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<QuotationListQuery>,
) -> AppResult<Json<ApiResponse<QuotationListResponse>>> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let response = service::list_quotations(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Quotations retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/", tag = modules::QUOTATIONS, request_body = CreateQuotationRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Quotation created", body = ApiResponse<Quotation>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Customer or product not found", body = ErrorResponse),
    )
)]
async fn create_quotation(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Json(body): Json<CreateQuotationRequest>,
) -> AppResult<Json<ApiResponse<Quotation>>> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let quotation =
        service::create_quotation(&state.db, user.tenant_id.as_deref(), body, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        quotation,
        "Quotation created successfully",
    )))
}

#[utoipa::path(get, path = "/{id}", tag = modules::QUOTATIONS,
    params(("id" = String, Path, description = "Quotation ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a quotation", body = ApiResponse<Quotation>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Quotation not found", body = ErrorResponse),
    )
)]
async fn get_quotation(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Quotation>>> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let quotation = service::get_quotation(&state.db, &id).await?;
    Ok(Json(ApiResponse::success(
        quotation,
        "Quotation retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/{id}/cancel", tag = modules::QUOTATIONS,
    params(("id" = String, Path, description = "Quotation ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Quotation cancelled", body = ApiResponse<Quotation>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Quotation not found", body = ErrorResponse),
        (status = 409, description = "QUOTATION_ALREADY_CANCELLED", body = ErrorResponse),
    )
)]
async fn cancel_quotation(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Quotation>>> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let quotation = service::cancel_quotation(&state.db, &id, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        quotation,
        "Quotation cancelled successfully",
    )))
}

#[derive(Debug, Deserialize)]
struct DocumentQueryParams {
    /// A specific design family for this one print. Omitted → the shop's
    /// chosen design for the quotation kind.
    template: Option<String>,
    /// `original` serves the PDF exactly as first printed; `current` prints
    /// with the design in force now. Required: the caller decides.
    design: DocumentDesignChoice,
}

/// Cancelling stamps the PDF, so a cancelled quotation must not be served the
/// cached active one (and vice versa) — the status is part of the key.
fn cache_key_for(quotation: &crate::domain::quotations::Quotation) -> &'static str {
    if quotation.status == STATUS_CANCELLED {
        "quotation-cancelled"
    } else {
        "quotation"
    }
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
async fn get_quotation_document(
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

#[derive(Debug, Deserialize)]
struct DesignStatusParams {
    template: Option<String>,
}

#[utoipa::path(get, path = "/{id}/document/design", tag = modules::QUOTATIONS,
    params(
        ("id" = String, Path, description = "Quotation ID or prefixed key"),
        ("template" = Option<String>, Query, description = "A design family for this print only"),
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "What the stored PDF was printed with versus what would print now", body = ApiResponse<DocumentDesignStatus>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Quotation not found", body = ErrorResponse),
        (status = 422, description = "TEMPLATE_UNAVAILABLE: the requested or chosen design is not available", body = ErrorResponse),
    )
)]
async fn get_quotation_document_design(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<DesignStatusParams>,
) -> AppResult<Json<ApiResponse<DocumentDesignStatus>>> {
    user.require_permission(perm::QUOTATIONS_MANAGE)?;
    let (quotation, _shop) = service::get_quotation_for_print(&state.db, &id).await?;
    let status = documents::service::print_design_status(
        &state.db,
        &state.document_server,
        "quotation",
        params.template.as_deref(),
        &quotation.key,
        cache_key_for(&quotation),
    )
    .await?;
    Ok(Json(ApiResponse::success(
        status,
        "Document design status retrieved successfully",
    )))
}
