use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::{AppError, AppResult};
use crate::core::middleware::auth::{AdminUser, CurrentUser};
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::core::utils::module_status_response;
use crate::domain::ModuleStatusResponse;
use crate::domain::billing::{
    BillingStats, CompleteSaleResponse, CreateCreditNoteRequest, CreateSaleRequest, CreditNote,
    CreditNoteListQuery, CreditNoteListResponse, Invoice, InvoiceListQuery, InvoiceListResponse,
    PaymentListResponse, PaymentRecord, RecordPaymentRequest, VoidCreditNoteRequest,
    VoidInvoiceRequest,
};
use crate::domain::settings::{DocumentDesignChoice, DocumentDesignStatus};
use crate::modules::billing::service;
use crate::modules::billing::service::sale;
use crate::modules::documents;
use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(post, path = "/invoices/{id}/close", tag = modules::BILLING,
    params(("id" = String, Path, description = "Invoice ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Invoice closed", body = ApiResponse<Invoice>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
        (status = 409, description = "Invoice is not Paid, or has an open (non-voided) credit note against it", body = ErrorResponse),
    )
)]
pub(super) async fn close_invoice(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Invoice>>> {
    let invoice = sale::close_invoice(&state.db, &id, admin.0.user_id).await?;
    Ok(Json(ApiResponse::success(
        invoice,
        "Invoice closed successfully",
    )))
}

#[utoipa::path(post, path = "/sales", tag = modules::BILLING, request_body = CreateSaleRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Sale completed — invoice and any payment(s) recorded", body = ApiResponse<CompleteSaleResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "customerKey does not resolve to an existing customer", body = ErrorResponse),
    )
)]
pub(super) async fn complete_sale(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Json(body): Json<CreateSaleRequest>,
) -> AppResult<Json<ApiResponse<CompleteSaleResponse>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let response = sale::complete_sale(&state.db, body, user.user_id, device_id.0).await?;
    state.reports_engine.invalidate_active();
    Ok(Json(ApiResponse::success(
        response,
        "Sale completed successfully",
    )))
}

#[utoipa::path(post, path = "/credit-notes", tag = modules::BILLING, request_body = CreateCreditNoteRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Credit note created — stock adjusted, refund/payment and customer balance updated", body = ApiResponse<CreditNote>),
        (status = 400, description = "Validation error or return quantity exceeded", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied, or manager override required (no-receipt / past return window)", body = ErrorResponse),
        (status = 404, description = "Invoice or product not found", body = ErrorResponse),
        (status = 409, description = "Invoice not eligible for a credit note, or past the return window without override", body = ErrorResponse),
    )
)]
pub(super) async fn create_credit_note(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Json(body): Json<CreateCreditNoteRequest>,
) -> AppResult<Json<ApiResponse<CreditNote>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let cashier_name = user.resolve_name(&state.db).await?;
    let response = service::credit_notes::create_credit_note(
        &state.db,
        &state.config,
        body,
        user.user_id,
        cashier_name,
        user.role,
        device_id.0,
    )
    .await?;
    state.reports_engine.invalidate_active();
    Ok(Json(ApiResponse::success(
        response,
        "Credit note processed successfully",
    )))
}

#[derive(Debug, Deserialize)]
pub(super) struct DesignStatusParams {
    #[serde(rename = "paperWidthMm")]
    pub(super) paper_width_mm: Option<u32>,
    pub(super) template: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct DocumentQueryParams {
    #[serde(rename = "paperWidthMm")]
    pub(super) paper_width_mm: Option<u32>,
    /// A specific design family for this one print. Omitted → the shop's
    /// chosen design for the document kind.
    pub(super) template: Option<String>,
    /// `original` serves the PDF exactly as first printed; `current` prints
    /// with the design in force now. Required: the caller decides.
    pub(super) design: DocumentDesignChoice,
}

#[utoipa::path(get, path = "/credit-notes/{id}", tag = modules::BILLING,
    params(("id" = String, Path, description = "Credit note ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a credit note record", body = ApiResponse<CreditNote>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Credit note not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_credit_note(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<CreditNote>>> {
    let credit_note = service::credit_notes::get_credit_note(&state.db, &id).await?;
    Ok(Json(ApiResponse::success(
        credit_note,
        "Credit note retrieved successfully",
    )))
}

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

#[utoipa::path(get, path = "/invoices/{id}", tag = modules::BILLING,
    params(("id" = String, Path, description = "Invoice ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get an invoice", body = ApiResponse<Invoice>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_invoice(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Invoice>>> {
    let invoice = service::get_invoice(&state.db, &id).await?;
    Ok(Json(ApiResponse::success(
        invoice,
        "Invoice retrieved successfully",
    )))
}

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

#[utoipa::path(get, path = "/invoices/stats", tag = modules::BILLING,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Sales & Invoices History KPI cards: today's sales, today's invoice count, outstanding credit, and average basket value", body = ApiResponse<BillingStats>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn get_invoice_stats(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<BillingStats>>> {
    let stats = service::get_billing_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Billing stats retrieved successfully",
    )))
}

/// The document kind and print-variant cache key of an invoice document:
/// `a4-invoice`, or `thermal-receipt` per paper width (default 80 mm).
pub(super) fn invoice_variant(
    document_type: &str,
    paper_width_mm: Option<u32>,
) -> AppResult<(&'static str, String)> {
    match document_type {
        "a4-invoice" => Ok(("a4-invoice", "a4-invoice".to_string())),
        "thermal-receipt" => {
            let width = paper_width_mm.unwrap_or(80);
            Ok(("thermal-receipt", format!("thermal-receipt-{width}mm")))
        }
        other => Err(AppError::validation(format!(
            "Unknown documentType '{other}'. Must be 'a4-invoice' or 'thermal-receipt'"
        ))),
    }
}

#[utoipa::path(get, path = "/credit-notes", tag = modules::BILLING, params(CreditNoteListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List credit note records with search, date/invoice/customer filtering, and pagination", body = ApiResponse<CreditNoteListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn list_credit_notes(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<CreditNoteListQuery>,
) -> AppResult<Json<ApiResponse<CreditNoteListResponse>>> {
    let response = service::credit_notes::list_credit_notes(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Credit notes retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/invoices", tag = modules::BILLING, params(InvoiceListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List invoices with search, status/customer filtering, and pagination", body = ApiResponse<InvoiceListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn list_invoices(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<InvoiceListQuery>,
) -> AppResult<Json<ApiResponse<InvoiceListResponse>>> {
    let response = service::list_invoices(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Invoices retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/invoices/{id}/payments", tag = modules::BILLING,
    params(("id" = String, Path, description = "Invoice ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List payments recorded against an invoice", body = ApiResponse<PaymentListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
    )
)]
pub(super) async fn list_payments(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<PaymentListResponse>>> {
    let invoice = service::get_invoice(&state.db, &id).await?;
    let payments = service::payments::list_payments(&state.db, &invoice.key).await?;
    Ok(Json(ApiResponse::success(
        PaymentListResponse { payments },
        "Payments retrieved successfully",
    )))
}

#[utoipa::path(post, path = "/invoices/{id}/payments", tag = modules::BILLING,
    params(("id" = String, Path, description = "Invoice ID or prefixed key")),
    request_body = RecordPaymentRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Payment recorded against the invoice", body = ApiResponse<PaymentRecord>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
        (status = 409, description = "Invoice already cancelled, or payment exceeds outstanding balance", body = ErrorResponse),
    )
)]
pub(super) async fn record_payment(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
    Json(body): Json<RecordPaymentRequest>,
) -> AppResult<Json<ApiResponse<PaymentRecord>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let invoice = service::get_invoice(&state.db, &id).await?;
    let recorded_by_name = user.resolve_name(&state.db).await?;
    let payment = service::payments::record_payment(
        &state.db,
        &invoice.key,
        body,
        user.user_id,
        recorded_by_name,
        device_id.0,
    )
    .await?;
    state.reports_engine.invalidate_active();
    Ok(Json(ApiResponse::success(
        payment,
        "Payment recorded successfully",
    )))
}

pub(super) fn require_credit_note_type(document_type: &str) -> AppResult<()> {
    if document_type != "credit-note" {
        return Err(AppError::validation(format!(
            "Unknown documentType '{document_type}'. Must be 'credit-note'"
        )));
    }
    Ok(())
}

/// Routes of the billing module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(status))
        .routes(routes!(complete_sale))
        .routes(routes!(list_invoices))
        .routes(routes!(get_invoice_stats))
        .routes(routes!(get_invoice))
        .routes(routes!(void_invoice))
        .routes(routes!(close_invoice))
        .routes(routes!(get_invoice_document))
        .routes(routes!(get_invoice_document_design))
        .routes(routes!(record_payment, list_payments))
        .routes(routes!(create_credit_note, list_credit_notes))
        .routes(routes!(get_credit_note))
        .routes(routes!(void_credit_note))
        .routes(routes!(get_credit_note_document))
        .routes(routes!(get_credit_note_document_design))
}

#[utoipa::path(get, path = "/", tag = modules::BILLING, responses(
    (status = 200, description = "Billing module status", body = ApiResponse<ModuleStatusResponse>)
))]
pub(super) async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::BILLING)
}

#[utoipa::path(post, path = "/credit-notes/{id}/void", tag = modules::BILLING,
    params(("id" = String, Path, description = "Credit note ID or prefixed key")),
    request_body = VoidCreditNoteRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Credit note voided — payments/customer balance/restock reversed where applicable", body = ApiResponse<CreditNote>),
        (status = 400, description = "A reason is required", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Credit note not found", body = ErrorResponse),
        (status = 409, description = "Already voided", body = ErrorResponse),
    )
)]
pub(super) async fn void_credit_note(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<VoidCreditNoteRequest>,
) -> AppResult<Json<ApiResponse<CreditNote>>> {
    let credit_note =
        service::credit_notes::void_credit_note(&state.db, &id, body, admin.0.user_id).await?;
    state.reports_engine.invalidate_active();
    Ok(Json(ApiResponse::success(
        credit_note,
        "Credit note voided successfully",
    )))
}

#[utoipa::path(post, path = "/invoices/{id}/void", tag = modules::BILLING,
    params(("id" = String, Path, description = "Invoice ID or prefixed key")),
    request_body = VoidInvoiceRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Invoice voided — stock and customer balance reversed where applicable", body = ApiResponse<Invoice>),
        (status = 400, description = "A reason is required", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
        (status = 409, description = "Already voided, or has payments recorded beyond the original sale", body = ErrorResponse),
    )
)]
pub(super) async fn void_invoice(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<VoidInvoiceRequest>,
) -> AppResult<Json<ApiResponse<Invoice>>> {
    let (invoice, warnings) = sale::void_invoice(&state.db, &id, body, admin.0.user_id).await?;
    state.reports_engine.invalidate_active();
    let message = if warnings.is_empty() {
        "Invoice voided successfully".to_string()
    } else {
        format!("Invoice voided with warnings: {}", warnings.join("; "))
    };
    Ok(Json(ApiResponse::success(invoice, message)))
}
