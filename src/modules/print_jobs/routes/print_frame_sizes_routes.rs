//! HTTP layer of the frame size list.

use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::constants::permissions as perm;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::middleware::sync_headers::DeviceId;
use crate::core::response::ApiResponse;
use crate::core::response::ErrorResponse;
use crate::domain::print_jobs::CreatePrintFrameSizeRequest;
use crate::domain::print_jobs::PrintFrameSize;
use crate::domain::print_jobs::PrintFrameSizeListQuery;
use crate::domain::print_jobs::UpdatePrintFrameSizeRequest;
use crate::modules::print_jobs::service;
use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(get, path = "/", tag = modules::PRINT_FRAME_SIZES,
    params(PrintFrameSizeListQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The shop's frame sizes", body = ApiResponse<Vec<PrintFrameSize>>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
    )
)]
/// Needs `print_jobs:write` (the print form offers them); hidden ones also need
/// `print_frame_sizes:manage`.
pub(super) async fn list_print_frame_sizes(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<PrintFrameSizeListQuery>,
) -> AppResult<Json<ApiResponse<Vec<PrintFrameSize>>>> {
    user.require_permission(perm::PRINT_JOBS_WRITE)?;
    let include_hidden = query.include_hidden == Some(true);
    if include_hidden {
        user.require_permission(perm::PRINT_FRAME_SIZES_MANAGE)?;
    }
    let prices = service::list_frame_sizes(&state.db, include_hidden).await?;
    Ok(Json(ApiResponse::success(prices, "Frame sizes")))
}

#[utoipa::path(post, path = "/", tag = modules::PRINT_FRAME_SIZES,
    request_body = CreatePrintFrameSizeRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Frame size added", body = ApiResponse<PrintFrameSize>),
        (status = 400, description = "Size or width is not valid", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 409, description = "This size and width is already listed", body = ErrorResponse),
    )
)]
/// Needs `print_frame_sizes:manage`.
pub(super) async fn create_print_frame_size(
    user: CurrentUser,
    State(state): State<AppState>,
    DeviceId(device_id): DeviceId,
    Json(body): Json<CreatePrintFrameSizeRequest>,
) -> AppResult<Json<ApiResponse<PrintFrameSize>>> {
    user.require_permission(perm::PRINT_FRAME_SIZES_MANAGE)?;
    let created = service::create_frame_size(&state.db, body, device_id).await?;
    Ok(Json(ApiResponse::success(created, "Frame size added")))
}

#[utoipa::path(put, path = "/{key}", tag = modules::PRINT_FRAME_SIZES,
    params(("key" = String, Path, description = "Frame size key (`pfs_…`)")),
    request_body = UpdatePrintFrameSizeRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Frame size saved", body = ApiResponse<PrintFrameSize>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Frame size not found", body = ErrorResponse),
        (status = 409, description = "It changed meanwhile", body = ErrorResponse),
    )
)]
/// Needs `print_frame_sizes:manage`. Hide a size with `active: false`; there is no delete.
pub(super) async fn update_print_frame_size(
    user: CurrentUser,
    State(state): State<AppState>,
    DeviceId(device_id): DeviceId,
    Path(key): Path<String>,
    Json(body): Json<UpdatePrintFrameSizeRequest>,
) -> AppResult<Json<ApiResponse<PrintFrameSize>>> {
    user.require_permission(perm::PRINT_FRAME_SIZES_MANAGE)?;
    let saved = service::update_frame_size(&state.db, &key, body, device_id).await?;
    Ok(Json(ApiResponse::success(saved, "Frame size saved")))
}

/// Routes of `/api/print-frame-sizes`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_print_frame_sizes, create_print_frame_size))
        .routes(routes!(update_print_frame_size))
}
