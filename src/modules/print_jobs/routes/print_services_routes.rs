use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::print_jobs::{PrintService, PrintServiceListQuery, SavePrintServiceRequest};
use crate::modules::print_jobs::service;
use axum::Json;
use axum::extract::{Path, Query, State};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(get, path = "/", tag = modules::PRINT_SERVICES,
    params(PrintServiceListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The shop's print services, by name", body = ApiResponse<Vec<PrintService>>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Needs `print_jobs:write` (the print form offers them); hidden ones also need
/// `print_services:manage`.
pub(super) async fn list_print_services(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<PrintServiceListQuery>,
) -> AppResult<Json<ApiResponse<Vec<PrintService>>>> {
    user.require_permission(perm::PRINT_JOBS_WRITE)?;
    let include_hidden = query.include_hidden == Some(true);
    if include_hidden {
        user.require_permission(perm::PRINT_SERVICES_MANAGE)?;
    }
    let services = service::list_services(&state.db, include_hidden).await?;
    Ok(Json(ApiResponse::success(services, "Print services")))
}

#[utoipa::path(post, path = "/", tag = modules::PRINT_SERVICES,
    request_body = SavePrintServiceRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Service added", body = ApiResponse<PrintService>),
        (status = 400, description = "Name, price or warranty is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 409, description = "Another service has this name", body = ErrorResponse),
    )
)]
/// Needs `print_services:manage`.
pub(super) async fn create_print_service(
    user: CurrentUser,
    State(state): State<AppState>,
    DeviceId(device_id): DeviceId,
    Json(body): Json<SavePrintServiceRequest>,
) -> AppResult<Json<ApiResponse<PrintService>>> {
    user.require_permission(perm::PRINT_SERVICES_MANAGE)?;
    let created = service::create_service(&state.db, body, device_id).await?;
    Ok(Json(ApiResponse::success(created, "Print service added")))
}

#[utoipa::path(put, path = "/{key}", tag = modules::PRINT_SERVICES,
    params(("key" = String, Path, description = "Service key (`pst_…`)")),
    request_body = SavePrintServiceRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Service saved", body = ApiResponse<PrintService>),
        (status = 400, description = "Name, price or warranty is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Service not found", body = ErrorResponse),
        (status = 409, description = "Another service has this name, or it changed meanwhile", body = ErrorResponse),
    )
)]
/// Needs `print_services:manage`. Hide a service with `active: false`; there is no delete.
pub(super) async fn update_print_service(
    user: CurrentUser,
    State(state): State<AppState>,
    DeviceId(device_id): DeviceId,
    Path(key): Path<String>,
    Json(body): Json<SavePrintServiceRequest>,
) -> AppResult<Json<ApiResponse<PrintService>>> {
    user.require_permission(perm::PRINT_SERVICES_MANAGE)?;
    let saved = service::update_service(&state.db, &key, body, device_id).await?;
    Ok(Json(ApiResponse::success(saved, "Print service saved")))
}

/// Routes of `/api/repair-services`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_print_services, create_print_service))
        .routes(routes!(update_print_service))
}
