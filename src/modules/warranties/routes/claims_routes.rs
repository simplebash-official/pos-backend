//! `/api/warranty-claims`: opening a claim and taking it through inspection, the manager's
//! decision, the supplier, the outcome and back to the customer.

use super::warranties_routes::actor;
use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::warranties::{
    ClaimListQuery, ClaimListResponse, DecideClaimRequest, InspectClaimRequest, OpenClaimRequest,
    ResolveClaimRequest, SendToSupplierRequest, SupplierReturnRequest, WarrantyClaim,
};
use crate::modules::warranties::service;
use axum::Json;
use axum::extract::{Path, Query, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(post, path = "/", tag = modules::WARRANTY_CLAIMS,
    request_body = OpenClaimRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Claim opened; the warranty is held until it is decided", body = ApiResponse<WarrantyClaim>),
        (status = 400, description = "No issue written, or an approval sent for a warranty in force", body = ErrorResponse),
        (status = 401, description = "Missing token, or the PIN is not a manager's or owner's", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Warranty or technician not found", body = ErrorResponse),
        (status = 409, description = "Cancelled, already under a claim, or run out without an approval", body = ErrorResponse),
    )
)]
/// Opens a claim. A warranty that has run out needs a manager or owner PIN and a reason.
pub(super) async fn open_claim(
    State(state): State<AppState>,
    user: CurrentUser,
    DeviceId(device_id): DeviceId,
    Json(body): Json<OpenClaimRequest>,
) -> AppResult<Json<ApiResponse<WarrantyClaim>>> {
    user.require_permission(perm::WARRANTY_CLAIMS_RECORD)?;
    let actor = actor(&state, &user).await?;
    let claim = service::open_claim(&state.db, body, &actor, device_id).await?;
    Ok(Json(ApiResponse::success(claim, "Claim opened")))
}

#[utoipa::path(get, path = "/", tag = modules::WARRANTY_CLAIMS,
    params(ClaimListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "A page of claims, oldest first", body = ApiResponse<ClaimListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Lists claims for the queue, a warranty, an invoice or a supplier.
pub(super) async fn list_claims(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<ClaimListQuery>,
) -> AppResult<Json<ApiResponse<ClaimListResponse>>> {
    user.require_permission(perm::WARRANTIES_VIEW)?;
    let page = service::list_claims(&state.db, query).await?;
    Ok(Json(ApiResponse::success(page, "Claims retrieved")))
}

#[utoipa::path(get, path = "/{key}", tag = modules::WARRANTY_CLAIMS,
    params(("key" = String, Path, description = "Claim key (`wcl_…`)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The claim", body = ApiResponse<WarrantyClaim>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Claim not found", body = ErrorResponse),
    )
)]
/// One claim.
pub(super) async fn get_claim(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<WarrantyClaim>>> {
    user.require_permission(perm::WARRANTIES_VIEW)?;
    let claim = service::get_claim(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(claim, "Claim retrieved")))
}

#[utoipa::path(post, path = "/{key}/inspect", tag = modules::WARRANTY_CLAIMS,
    params(("key" = String, Path, description = "Claim key (`wcl_…`)")),
    request_body = InspectClaimRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Claim inspected", body = ApiResponse<WarrantyClaim>),
        (status = 400, description = "No notes written", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Claim not found", body = ErrorResponse),
        (status = 409, description = "Not allowed from where the claim is, or changed by someone else meanwhile", body = ErrorResponse),
    )
)]
/// Records what the inspection found.
pub(super) async fn inspect_claim(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<InspectClaimRequest>,
) -> AppResult<Json<ApiResponse<WarrantyClaim>>> {
    user.require_permission(perm::WARRANTY_CLAIMS_RECORD)?;
    let actor = actor(&state, &user).await?;
    let claim = service::inspect_claim(&state.db, &key, body, &actor).await?;
    Ok(Json(ApiResponse::success(claim, "Claim inspected")))
}

#[utoipa::path(post, path = "/{key}/decide", tag = modules::WARRANTY_CLAIMS,
    params(("key" = String, Path, description = "Claim key (`wcl_…`)")),
    request_body = DecideClaimRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Claim decided", body = ApiResponse<WarrantyClaim>),
        (status = 400, description = "No reason given", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Claim not found", body = ErrorResponse),
        (status = 409, description = "Not allowed from where the claim is, or changed by someone else meanwhile", body = ErrorResponse),
    )
)]
/// A manager or the owner decides: repair, replace, refund or reject (PIN and reason).
pub(super) async fn decide_claim(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<DecideClaimRequest>,
) -> AppResult<Json<ApiResponse<WarrantyClaim>>> {
    user.require_permission(perm::WARRANTY_CLAIMS_RECORD)?;
    let actor = actor(&state, &user).await?;
    let claim = service::decide_claim(&state.db, &key, body, &actor).await?;
    Ok(Json(ApiResponse::success(claim, "Claim decided")))
}

#[utoipa::path(post, path = "/{key}/supplier/send", tag = modules::WARRANTY_CLAIMS,
    params(("key" = String, Path, description = "Claim key (`wcl_…`)")),
    request_body = SendToSupplierRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Sent to the supplier", body = ApiResponse<WarrantyClaim>),
        (status = 400, description = "The expected return day is not a date or is past", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Claim not found", body = ErrorResponse),
        (status = 409, description = "Not allowed from where the claim is, or changed by someone else meanwhile", body = ErrorResponse),
    )
)]
/// Sends the item on to a supplier or maker.
pub(super) async fn send_to_supplier(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<SendToSupplierRequest>,
) -> AppResult<Json<ApiResponse<WarrantyClaim>>> {
    user.require_permission(perm::WARRANTY_CLAIMS_RECORD)?;
    let actor = actor(&state, &user).await?;
    let claim = service::send_to_supplier(&state.db, &key, body, &actor).await?;
    Ok(Json(ApiResponse::success(claim, "Sent to the supplier")))
}

#[utoipa::path(post, path = "/{key}/supplier/return", tag = modules::WARRANTY_CLAIMS,
    params(("key" = String, Path, description = "Claim key (`wcl_…`)")),
    request_body = SupplierReturnRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Back from the supplier", body = ApiResponse<WarrantyClaim>),
        (status = 400, description = "Malformed request", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Claim not found", body = ErrorResponse),
        (status = 409, description = "Not allowed from where the claim is, or changed by someone else meanwhile", body = ErrorResponse),
    )
)]
/// Records that the item is back from the supplier and what they did.
pub(super) async fn supplier_return(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<SupplierReturnRequest>,
) -> AppResult<Json<ApiResponse<WarrantyClaim>>> {
    user.require_permission(perm::WARRANTY_CLAIMS_RECORD)?;
    let actor = actor(&state, &user).await?;
    let claim = service::supplier_return(&state.db, &key, body, &actor).await?;
    Ok(Json(ApiResponse::success(claim, "Back from the supplier")))
}

#[utoipa::path(post, path = "/{key}/resolve", tag = modules::WARRANTY_CLAIMS,
    params(("key" = String, Path, description = "Claim key (`wcl_…`)")),
    request_body = ResolveClaimRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Claim resolved", body = ApiResponse<WarrantyClaim>),
        (status = 400, description = "A replacement sent for a repair", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Claim not found", body = ErrorResponse),
        (status = 409, description = "Not allowed from where the claim is, or changed by someone else meanwhile", body = ErrorResponse),
    )
)]
/// Finishes a repair, or gives the replacement unit (taken from stock).
pub(super) async fn resolve_claim(
    State(state): State<AppState>,
    user: CurrentUser,
    DeviceId(device_id): DeviceId,
    Path(key): Path<String>,
    Json(body): Json<ResolveClaimRequest>,
) -> AppResult<Json<ApiResponse<WarrantyClaim>>> {
    user.require_permission(perm::WARRANTY_CLAIMS_RECORD)?;
    let actor = actor(&state, &user).await?;
    let claim = service::resolve_claim(&state.db, &key, body, &actor, device_id).await?;
    Ok(Json(ApiResponse::success(claim, "Claim resolved")))
}

#[utoipa::path(post, path = "/{key}/return", tag = modules::WARRANTY_CLAIMS,
    params(("key" = String, Path, description = "Claim key (`wcl_…`)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Given back to the customer", body = ApiResponse<WarrantyClaim>),
        (status = 400, description = "Malformed request", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Claim not found", body = ErrorResponse),
        (status = 409, description = "Not allowed from where the claim is, or changed by someone else meanwhile", body = ErrorResponse),
    )
)]
/// Hands the item back to the customer.
pub(super) async fn return_to_customer(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<WarrantyClaim>>> {
    user.require_permission(perm::WARRANTY_CLAIMS_RECORD)?;
    let actor = actor(&state, &user).await?;
    let claim = service::return_to_customer(&state.db, &key, &actor).await?;
    Ok(Json(ApiResponse::success(
        claim,
        "Given back to the customer",
    )))
}

#[utoipa::path(post, path = "/{key}/paid-repair", tag = modules::WARRANTY_CLAIMS,
    params(("key" = String, Path, description = "Claim key (`wcl_…`)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Paid repair opened", body = ApiResponse<WarrantyClaim>),
        (status = 400, description = "The customer has no name or phone for a repair job", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Claim not found", body = ErrorResponse),
        (status = 409, description = "Not allowed from where the claim is, or changed by someone else meanwhile", body = ErrorResponse),
    )
)]
/// Opens a paid repair job for an item whose claim was rejected.
pub(super) async fn convert_to_paid_repair(
    State(state): State<AppState>,
    user: CurrentUser,
    DeviceId(device_id): DeviceId,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<WarrantyClaim>>> {
    user.require_permission(perm::WARRANTY_CLAIMS_RECORD)?;
    user.require_permission(perm::REPAIRS_WRITE)?;
    let actor = actor(&state, &user).await?;
    let claim = service::convert_to_paid_repair(&state.db, &key, &actor, device_id).await?;
    Ok(Json(ApiResponse::success(claim, "Paid repair opened")))
}

#[utoipa::path(post, path = "/{key}/rework", tag = modules::WARRANTY_CLAIMS,
    params(("key" = String, Path, description = "Claim key (`wcl_…`)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Rework job opened", body = ApiResponse<WarrantyClaim>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Claim, warranty or original job not found", body = ErrorResponse),
        (status = 409, description = "Not decided as a repair, already resolved or reworked, or the warranty is not on repair work", body = ErrorResponse),
    )
)]
/// Opens the free rework job for a claim on repair work decided as a repair.
pub(super) async fn open_rework(
    State(state): State<AppState>,
    user: CurrentUser,
    DeviceId(device_id): DeviceId,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<WarrantyClaim>>> {
    user.require_permission(perm::WARRANTY_CLAIMS_RECORD)?;
    user.require_permission(perm::REPAIRS_WRITE)?;
    let actor = actor(&state, &user).await?;
    let claim = service::open_rework(&state.db, &key, &actor, device_id).await?;
    Ok(Json(ApiResponse::success(claim, "Rework job opened")))
}

/// Routes of `/api/warranty-claims`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads.
        .routes(routes!(open_claim, list_claims))
        .routes(routes!(get_claim))
        // Steps, in lifecycle order.
        .routes(routes!(inspect_claim))
        .routes(routes!(decide_claim))
        .routes(routes!(send_to_supplier))
        .routes(routes!(supplier_return))
        .routes(routes!(resolve_claim))
        .routes(routes!(return_to_customer))
        .routes(routes!(convert_to_paid_repair))
        .routes(routes!(open_rework))
}
