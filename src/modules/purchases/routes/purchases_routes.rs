use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::purchase_receipts::{
    CreateReceiptRequest, PaymentSource, PurchaseReceipt, ReceiptListQuery, ReceiptListResponse,
};
use crate::domain::purchases::{PurchaseListQuery, PurchaseListResponse};
use crate::domain::supplier_credits::{
    CreateSupplierCreditRequest, SupplierCredit, SupplierCreditListQuery,
    SupplierCreditListResponse,
};
use crate::modules::purchases::service::receipt::RecordedBy;
use crate::modules::purchases::service::{credit, purchase, receipt};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Mounted at `/supplier-credits`.
pub fn credits_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_credits, record_credit))
        .routes(routes!(void_credit))
}

#[utoipa::path(get, path = "/{key}", tag = modules::PURCHASE_RECEIPTS,
    params(("key" = String, Path, description = "Receipt key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "One receipt with its lines", body = ApiResponse<PurchaseReceipt>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Receipt not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_receipt(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<PurchaseReceipt>>> {
    let response = receipt::get_receipt(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Receipt retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/", tag = modules::SUPPLIER_CREDITS, params(SupplierCreditListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Supplier credits, newest first", body = ApiResponse<SupplierCreditListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn list_credits(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<SupplierCreditListQuery>,
) -> AppResult<Json<ApiResponse<SupplierCreditListResponse>>> {
    let response = credit::list_credits(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Credits retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/", tag = modules::PURCHASES, params(PurchaseListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Purchase history for a supplier and/or product, or full collection paginated", body = ApiResponse<PurchaseListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn list_purchases(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<PurchaseListQuery>,
) -> AppResult<Json<ApiResponse<PurchaseListResponse>>> {
    let response = purchase::list_purchases(&state.db, query).await?;

    Ok(Json(ApiResponse::success(
        response,
        "Purchases retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/", tag = modules::PURCHASE_RECEIPTS, params(ReceiptListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Supplier receipts, newest first", body = ApiResponse<ReceiptListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
    )
)]
pub(super) async fn list_receipts(
    _admin: AdminUser,
    State(state): State<AppState>,
    Query(query): Query<ReceiptListQuery>,
) -> AppResult<Json<ApiResponse<ReceiptListResponse>>> {
    let response = receipt::list_receipts(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Receipts retrieved successfully",
    )))
}

/// Mounted at `/purchase-receipts`.
pub fn receipts_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_receipts, record_receipt))
        .routes(routes!(get_receipt))
}

#[utoipa::path(post, path = "/", tag = modules::SUPPLIER_CREDITS, request_body = CreateSupplierCreditRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Credit recorded", body = ApiResponse<SupplierCredit>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier or receipt not found", body = ErrorResponse),
        (status = 409, description = "More than the receipt still owes", body = ErrorResponse),
    )
)]
pub(super) async fn record_credit(
    AdminUser(current_user): AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateSupplierCreditRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<SupplierCredit>>)> {
    let by = RecordedBy {
        name: current_user.resolve_name(&state.db).await?,
        user_id: current_user.user_id,
    };
    let recorded = credit::create_credit(&state.db, by, body).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            recorded,
            "Credit recorded successfully",
        )),
    ))
}

#[utoipa::path(post, path = "/", tag = modules::PURCHASE_RECEIPTS, request_body = CreateReceiptRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Stock received and recorded", body = ApiResponse<PurchaseReceipt>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Supplier or product not found", body = ErrorResponse),
        (status = 409, description = "Duplicate invoice number or serial number", body = ErrorResponse),
    )
)]
pub(super) async fn record_receipt(
    AdminUser(current_user): AdminUser,
    State(state): State<AppState>,
    device_id: DeviceId,
    Json(body): Json<CreateReceiptRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<PurchaseReceipt>>)> {
    // What is paid as the stock arrives needs the permissions paying the supplier does.
    if let Some(payment) = &body.payment {
        match payment.source {
            PaymentSource::Drawer | PaymentSource::OwnerPocket => {
                current_user.require_permission(perm::CASH_MOVEMENTS_CREATE)?;
                current_user.require_permission(perm::CASH_MOVEMENTS_CREATE_MANAGED)?;
            }
            PaymentSource::Bank | PaymentSource::Card | PaymentSource::Online => {
                current_user.require_permission(perm::EXPENSES_MANAGE)?;
            }
        }
    }
    let recorded_by = RecordedBy {
        name: current_user.resolve_name(&state.db).await?,
        user_id: current_user.user_id,
    };
    let recorded = receipt::record_receipt(&state.db, Some(recorded_by), body, device_id.0).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            recorded,
            "Stock received successfully",
        )),
    ))
}

/// Routes of the purchases module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list_purchases))
}

#[utoipa::path(post, path = "/{key}/void", tag = modules::SUPPLIER_CREDITS,
    params(("key" = String, Path, description = "Credit key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Credit voided by a reversing entry", body = ApiResponse<SupplierCredit>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Credit not found", body = ErrorResponse),
        (status = 409, description = "Already voided", body = ErrorResponse),
    )
)]
pub(super) async fn void_credit(
    AdminUser(current_user): AdminUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> AppResult<(StatusCode, Json<ApiResponse<SupplierCredit>>)> {
    let by = RecordedBy {
        name: current_user.resolve_name(&state.db).await?,
        user_id: current_user.user_id,
    };
    let voided = credit::void_credit(&state.db, by, &key).await?;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(voided, "Credit voided successfully")),
    ))
}
