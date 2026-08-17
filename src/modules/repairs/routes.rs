// HTTP layer only — same shape as `modules::customers::routes`. Reads
// require only a valid token; writes additionally require `REPAIRS_WRITE`
// (first route in the codebase to actually enforce that permission — it was
// defined as a placeholder in `core::constants::permissions` until now).

use axum::{
    Json,
    extract::{Path, Query, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::{modules, permissions as perm},
        error::AppResult,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::repairs::{
        CreateRepairRequest, Repair, RepairListQuery, RepairListResponse, UpdateRepairRequest,
    },
    modules::repairs::service,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_repairs, create_repair))
        .routes(routes!(get_repair, update_repair, delete_repair))
}

#[utoipa::path(get, path = "/", tag = modules::REPAIRS, params(RepairListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "List repair tickets with search, status filtering, and pagination", body = ApiResponse<RepairListResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    )
)]
async fn list_repairs(
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

#[utoipa::path(get, path = "/{id}", tag = modules::REPAIRS,
    params(("id" = String, Path, description = "Repair ticket ID or prefixed key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Get a repair ticket", body = ApiResponse<Repair>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 404, description = "Repair ticket not found", body = ErrorResponse),
    )
)]
async fn get_repair(
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

#[utoipa::path(post, path = "/", tag = modules::REPAIRS, request_body = CreateRepairRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Repair ticket created", body = ApiResponse<Repair>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
async fn create_repair(
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
async fn update_repair(
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
async fn delete_repair(
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
