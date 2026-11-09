use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::warranties::{
    ExtendWarrantyRequest, VoidWarrantyRequest, WarrantyDetail, WarrantyListQuery,
    WarrantyListResponse, WarrantyLookupQuery, WarrantyLookupResponse, WarrantyStats,
};
use crate::modules::warranties::service::{self, WarrantyActor};
use axum::Json;
use axum::extract::{Path, Query, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) async fn actor(state: &AppState, user: &CurrentUser) -> AppResult<WarrantyActor> {
    Ok(WarrantyActor {
        user_id: user.user_id.clone(),
        name: user.resolve_name(&state.db).await?,
    })
}

#[utoipa::path(get, path = "/lookup", tag = modules::WARRANTIES,
    params(WarrantyLookupQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Warranties grouped by device, newest first, and what the text matched", body = ApiResponse<WarrantyLookupResponse>),
        (status = 400, description = "Nothing was typed", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Finds warranties from a scanned or typed warranty number, serial / IMEI, invoice number or
/// phone number.
pub(super) async fn lookup_warranties(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WarrantyLookupQuery>,
) -> AppResult<Json<ApiResponse<WarrantyLookupResponse>>> {
    user.require_permission(perm::WARRANTIES_VIEW)?;
    let found = service::lookup(&state.db, &query.q).await?;
    Ok(Json(ApiResponse::success(found, "Warranties found")))
}

#[utoipa::path(get, path = "/stats", tag = modules::WARRANTIES,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The warranty desk at a glance: in force, ending this week, open and late claims, this month's rework cost", body = ApiResponse<WarrantyStats>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// The headline numbers for the Warranties page and the dashboard.
pub(super) async fn get_warranty_stats(
    State(state): State<AppState>,
    user: CurrentUser,
) -> AppResult<Json<ApiResponse<WarrantyStats>>> {
    user.require_permission(perm::WARRANTIES_VIEW)?;
    let stats = service::stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Warranty stats retrieved",
    )))
}

#[utoipa::path(get, path = "/", tag = modules::WARRANTIES,
    params(WarrantyListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "A page of warranties, newest first (ending soonest first for `state=expiring_soon`)", body = ApiResponse<WarrantyListResponse>),
        (status = 400, description = "More than one of customer, invoice, repair job and state was given", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Lists warranties, for a customer's, an invoice's or a job's page, or those ending this week.
pub(super) async fn list_warranties(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(query): Query<WarrantyListQuery>,
) -> AppResult<Json<ApiResponse<WarrantyListResponse>>> {
    user.require_permission(perm::WARRANTIES_VIEW)?;
    let page = service::list(&state.db, query).await?;
    Ok(Json(ApiResponse::success(page, "Warranties retrieved")))
}

#[utoipa::path(get, path = "/{key}", tag = modules::WARRANTIES,
    params(("key" = String, Path, description = "Warranty key (`wty_…`)")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The warranty, its activity log, its invoice and its replacement chain", body = ApiResponse<WarrantyDetail>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Warranty not found", body = ErrorResponse),
    )
)]
/// One warranty in full.
pub(super) async fn get_warranty(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<WarrantyDetail>>> {
    user.require_permission(perm::WARRANTIES_VIEW)?;
    let detail = service::detail(&state.db, &key).await?;
    Ok(Json(ApiResponse::success(detail, "Warranty retrieved")))
}

#[utoipa::path(post, path = "/{key}/void", tag = modules::WARRANTIES,
    params(("key" = String, Path, description = "Warranty key (`wty_…`)")),
    request_body = VoidWarrantyRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Cancelled", body = ApiResponse<WarrantyDetail>),
        (status = 400, description = "No reason given", body = ErrorResponse),
        (status = 401, description = "Missing token, or the PIN is not a manager's or owner's", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Warranty not found", body = ErrorResponse),
        (status = 409, description = "Already cancelled, or changed by someone else meanwhile", body = ErrorResponse),
    )
)]
/// Cancels a warranty; a manager or owner PIN and a reason are required.
pub(super) async fn void_warranty(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<VoidWarrantyRequest>,
) -> AppResult<Json<ApiResponse<WarrantyDetail>>> {
    user.require_permission(perm::WARRANTY_CLAIMS_RECORD)?;
    let actor = actor(&state, &user).await?;
    let caller = user.approval_caller(&state.db).await?;
    let detail = service::void(&state.db, &key, body, &actor, &caller).await?;
    Ok(Json(ApiResponse::success(detail, "Warranty cancelled")))
}

#[utoipa::path(post, path = "/{key}/extend", tag = modules::WARRANTIES,
    params(("key" = String, Path, description = "Warranty key (`wty_…`)")),
    request_body = ExtendWarrantyRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Extended", body = ApiResponse<WarrantyDetail>),
        (status = 400, description = "No reason, or the period is out of range", body = ErrorResponse),
        (status = 401, description = "Missing token, or the PIN is not a manager's or owner's", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Warranty not found", body = ErrorResponse),
        (status = 409, description = "Cancelled, already for life, not later than now, or changed meanwhile", body = ErrorResponse),
    )
)]
/// Lengthens a warranty from its original first day; a manager or owner PIN and a reason are
/// required.
pub(super) async fn extend_warranty(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(key): Path<String>,
    Json(body): Json<ExtendWarrantyRequest>,
) -> AppResult<Json<ApiResponse<WarrantyDetail>>> {
    user.require_permission(perm::WARRANTY_CLAIMS_RECORD)?;
    let actor = actor(&state, &user).await?;
    let caller = user.approval_caller(&state.db).await?;
    let detail = service::extend(&state.db, &key, body, &actor, &caller).await?;
    Ok(Json(ApiResponse::success(detail, "Warranty extended")))
}

/// Routes of the warranties module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads: the lookup and the stats are static so they never collide with `/{key}`.
        .routes(routes!(lookup_warranties))
        .routes(routes!(get_warranty_stats))
        .routes(routes!(list_warranties))
        .routes(routes!(get_warranty))
        // Manager-approved changes.
        .routes(routes!(void_warranty))
        .routes(routes!(extend_warranty))
        // The printed card.
        .routes(routes!(super::warranty_card_routes::get_warranty_card))
}
