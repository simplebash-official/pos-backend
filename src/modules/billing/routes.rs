// HTTP layer for billing: sale completion, invoice/payment reads, payment
// recording, cancellation, and on-demand document (PDF) fetching. The
// module-status stub at `GET /` is unrelated to any of that and stays as
// the intentionally-public placeholder it always was (see `app.rs`'s
// `PUBLIC_ROUTES` note) — `/invoices` is the real "list" entry point, not
// `/`, since billing's root has no natural single-collection meaning the
// way `suppliers`'/`customers`' root does.

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
        error::{AppError, AppResult},
        middleware::auth::{AdminUser, CurrentUser},
        response::{ApiResponse, ErrorResponse},
        utils::module_status_response,
    },
    domain::{
        ModuleStatusResponse,
        billing::{
            CancelInvoiceRequest, CompleteSaleResponse, CreateSaleRequest, Invoice,
            InvoiceListQuery, InvoiceListResponse, PaymentListResponse, PaymentRecord,
            RecordPaymentRequest,
        },
    },
    modules::{
        billing::service::{self, sale},
        documents, users,
    },
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(status))
        .routes(routes!(complete_sale))
        .routes(routes!(list_invoices))
        .routes(routes!(get_invoice))
        .routes(routes!(cancel_invoice))
        .routes(routes!(get_invoice_document))
        .routes(routes!(record_payment, list_payments))
}

#[utoipa::path(get, path = "/", tag = modules::BILLING, responses(
    (status = 200, description = "Billing module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::BILLING)
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
async fn complete_sale(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Json(body): Json<CreateSaleRequest>,
) -> AppResult<Json<ApiResponse<CompleteSaleResponse>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let response = sale::complete_sale(&state.db, body, user.user_id, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Sale completed successfully",
    )))
}

#[utoipa::path(get, path = "/invoices", tag = modules::BILLING, params(InvoiceListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List invoices with search, status/customer filtering, and pagination", body = ApiResponse<InvoiceListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
async fn list_invoices(
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

#[utoipa::path(get, path = "/invoices/{id}", tag = modules::BILLING,
    params(("id" = String, Path, description = "Invoice ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get an invoice", body = ApiResponse<Invoice>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
    )
)]
async fn get_invoice(
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

#[utoipa::path(post, path = "/invoices/{id}/cancel", tag = modules::BILLING,
    params(("id" = String, Path, description = "Invoice ID or prefixed key")),
    request_body = CancelInvoiceRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Invoice cancelled — stock and customer balance reversed where applicable", body = ApiResponse<Invoice>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
        (status = 409, description = "Already cancelled, or has payments recorded beyond the original sale", body = ErrorResponse),
    )
)]
async fn cancel_invoice(
    admin: AdminUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CancelInvoiceRequest>,
) -> AppResult<Json<ApiResponse<Invoice>>> {
    let (invoice, warnings) = sale::cancel_invoice(&state.db, &id, body, admin.0.user_id).await?;
    let message = if warnings.is_empty() {
        "Invoice cancelled successfully".to_string()
    } else {
        format!("Invoice cancelled with warnings: {}", warnings.join("; "))
    };
    Ok(Json(ApiResponse::success(invoice, message)))
}

#[derive(Debug, Deserialize)]
struct DocumentQueryParams {
    #[serde(rename = "paperWidthMm")]
    paper_width_mm: Option<u32>,
}

#[utoipa::path(get, path = "/invoices/{id}/documents/{documentType}", tag = modules::BILLING,
    params(
        ("id" = String, Path, description = "Invoice ID or prefixed key"),
        ("documentType" = String, Path, description = "'a4-invoice' or 'thermal-receipt'"),
        ("paperWidthMm" = Option<u32>, Query, description = "58 or 80 — thermal-receipt only, defaults to 80 (D9)"),
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Rendered (or cached) PDF", content_type = "application/pdf", body = Vec<u8>),
        (status = 400, description = "Unknown documentType", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Invoice not found", body = ErrorResponse),
    )
)]
async fn get_invoice_document(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path((id, document_type)): Path<(String, String)>,
    Query(params): Query<DocumentQueryParams>,
) -> AppResult<Response> {
    let invoice = service::get_invoice(&state.db, &id).await?;

    let (cache_key, template_name, data) = match document_type.as_str() {
        "a4-invoice" => (
            "a4-invoice".to_string(),
            "a4-invoice",
            service::print_payload::build_a4_invoice_data(
                &invoice,
                "ORIGINAL — CUSTOMER COPY",
                false,
            ),
        ),
        "thermal-receipt" => {
            let width = params.paper_width_mm.unwrap_or(80);
            (
                format!("thermal-receipt-{width}mm"),
                "thermal-receipt",
                service::print_payload::build_thermal_receipt_data(&invoice, width),
            )
        }
        other => {
            return Err(AppError::validation(format!(
                "Unknown documentType '{other}'. Must be 'a4-invoice' or 'thermal-receipt'"
            )));
        }
    };

    let pdf_bytes = documents::service::get_or_render(
        &state.db,
        &state.document_server,
        &state.config.generated_documents_dir,
        &invoice.key,
        &cache_key,
        template_name,
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
async fn record_payment(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<RecordPaymentRequest>,
) -> AppResult<Json<ApiResponse<PaymentRecord>>> {
    user.require_permission(perm::BILLING_WRITE)?;
    let invoice = service::get_invoice(&state.db, &id).await?;
    let recorded_by_name = match mongodb::bson::oid::ObjectId::parse_str(&user.user_id) {
        Ok(object_id) => users::service::get_user(&state.db, object_id)
            .await
            .map(|u| u.name)
            .unwrap_or_else(|_| "Unknown".to_string()),
        Err(_) => "Unknown".to_string(),
    };
    let payment = service::payments::record_payment(
        &state.db,
        &invoice.key,
        body,
        user.user_id,
        recorded_by_name,
    )
    .await?;
    Ok(Json(ApiResponse::success(
        payment,
        "Payment recorded successfully",
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
async fn list_payments(
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
