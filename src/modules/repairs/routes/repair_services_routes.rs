use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::repairs::{RepairService, RepairServiceListQuery, SaveRepairServiceRequest};
use crate::modules::repairs::service;
use axum::Json;
use axum::extract::{Path, Query, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(get, path = "/", tag = modules::REPAIR_SERVICES,
    params(RepairServiceListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The shop's repair services, by name", body = ApiResponse<Vec<RepairService>>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Needs `repairs:write` (the repair form offers them); hidden ones also need
/// `repair_services:manage`.
pub(super) async fn list_repair_services(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<RepairServiceListQuery>,
) -> AppResult<Json<ApiResponse<Vec<RepairService>>>> {
    user.require_permission(perm::REPAIRS_WRITE)?;
    let include_hidden = query.include_hidden == Some(true);
    if include_hidden {
        user.require_permission(perm::REPAIR_SERVICES_MANAGE)?;
    }
    let services = service::list_services(&state.db, include_hidden).await?;
    Ok(Json(ApiResponse::success(services, "Repair services")))
}

#[utoipa::path(post, path = "/", tag = modules::REPAIR_SERVICES,
    request_body = SaveRepairServiceRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Service added", body = ApiResponse<RepairService>),
        (status = 400, description = "Name, labour or warranty is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 409, description = "Another service has this name", body = ErrorResponse),
    )
)]
/// Needs `repair_services:manage`.
pub(super) async fn create_repair_service(
    user: CurrentUser,
    State(state): State<AppState>,
    DeviceId(device_id): DeviceId,
    Json(body): Json<SaveRepairServiceRequest>,
) -> AppResult<Json<ApiResponse<RepairService>>> {
    user.require_permission(perm::REPAIR_SERVICES_MANAGE)?;
    let created = service::create_service(&state.db, body, device_id).await?;
    Ok(Json(ApiResponse::success(created, "Repair service added")))
}

#[utoipa::path(put, path = "/{key}", tag = modules::REPAIR_SERVICES,
    params(("key" = String, Path, description = "Service key (`rst_…`)")),
    request_body = SaveRepairServiceRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Service saved", body = ApiResponse<RepairService>),
        (status = 400, description = "Name, labour or warranty is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Service not found", body = ErrorResponse),
        (status = 409, description = "Another service has this name, or it changed meanwhile", body = ErrorResponse),
    )
)]
/// Needs `repair_services:manage`. Hide a service with `active: false`; there is no delete.
pub(super) async fn update_repair_service(
    user: CurrentUser,
    State(state): State<AppState>,
    DeviceId(device_id): DeviceId,
    Path(key): Path<String>,
    Json(body): Json<SaveRepairServiceRequest>,
) -> AppResult<Json<ApiResponse<RepairService>>> {
    user.require_permission(perm::REPAIR_SERVICES_MANAGE)?;
    let saved = service::update_service(&state.db, &key, body, device_id).await?;
    Ok(Json(ApiResponse::success(saved, "Repair service saved")))
}

/// Routes of `/api/repair-services`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_repair_services, create_repair_service))
        .routes(routes!(update_repair_service))
}
