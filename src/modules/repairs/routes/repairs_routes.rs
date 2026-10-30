use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::repairs::{
    CreateRepairRequest, Repair, RepairListQuery, RepairListResponse, RepairStats,
    UpdateRepairRequest,
};
use crate::modules::repairs::service;
use axum::Json;
use axum::extract::{Path, Query, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(post, path = "/", tag = modules::REPAIRS, request_body = CreateRepairRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Repair ticket created", body = ApiResponse<Repair>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
pub(super) async fn create_repair(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Json(body): Json<CreateRepairRequest>,
) -> AppResult<Json<ApiResponse<Repair>>> {
    user.require_permission(perm::REPAIRS_WRITE)?;
    let repair = service::create_repair(&state.db, body, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        repair,
        "Repair ticket created successfully",
    )))
}

#[utoipa::path(delete, path = "/{id}", tag = modules::REPAIRS,
    params(("id" = String, Path, description = "Repair ticket ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Repair ticket deleted", body = ApiResponse<Repair>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Repair ticket not found", body = ErrorResponse),
    )
)]
pub(super) async fn delete_repair(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Repair>>> {
    user.require_permission(perm::REPAIRS_WRITE)?;
    let repair = service::delete_repair(&state.db, &id, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        repair,
        "Repair ticket deleted successfully",
    )))
}

#[utoipa::path(get, path = "/{id}", tag = modules::REPAIRS,
    params(("id" = String, Path, description = "Repair ticket ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a repair ticket", body = ApiResponse<Repair>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Repair ticket not found", body = ErrorResponse),
    )
)]
pub(super) async fn get_repair(
    _user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> AppResult<Json<ApiResponse<Repair>>> {
    let repair = service::get_repair(&state.db, &id).await?;
    Ok(Json(ApiResponse::success(
        repair,
        "Repair ticket retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/stats", tag = modules::REPAIRS,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Repair Jobs KPI cards: today's job count, today's revenue, open ticket count, and average job value", body = ApiResponse<RepairStats>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn get_repair_stats(
    _user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<RepairStats>>> {
    let stats = service::get_repair_stats(&state.db).await?;
    Ok(Json(ApiResponse::success(
        stats,
        "Repair stats retrieved successfully",
    )))
}

#[utoipa::path(get, path = "/", tag = modules::REPAIRS, params(RepairListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List repair tickets with search, status filtering, and pagination", body = ApiResponse<RepairListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
pub(super) async fn list_repairs(
    _user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<RepairListQuery>,
) -> AppResult<Json<ApiResponse<RepairListResponse>>> {
    let response = service::list_repairs(&state.db, query).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Repair tickets retrieved successfully",
    )))
}

/// Routes of the repairs module.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_repairs, create_repair))
        .routes(routes!(get_repair_stats))
        .routes(routes!(get_repair, update_repair, delete_repair))
}

#[utoipa::path(patch, path = "/{id}", tag = modules::REPAIRS,
    params(("id" = String, Path, description = "Repair ticket ID or prefixed key")),
    request_body = UpdateRepairRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Repair ticket updated", body = ApiResponse<Repair>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Repair ticket not found", body = ErrorResponse),
    )
)]
pub(super) async fn update_repair(
    user: CurrentUser,
    State(state): State<AppState>,
    device_id: crate::core::middleware::sync_headers::DeviceId,
    Path(id): Path<String>,
    Json(body): Json<UpdateRepairRequest>,
) -> AppResult<Json<ApiResponse<Repair>>> {
    user.require_permission(perm::REPAIRS_WRITE)?;
    let repair = service::update_repair(&state.db, &id, body, device_id.0).await?;
    Ok(Json(ApiResponse::success(
        repair,
        "Repair ticket updated successfully",
    )))
}
