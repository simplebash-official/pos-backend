use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::repairs::{Repair, UpdateRepairRequest};
use crate::modules::repairs::service;
use axum::Json;
use axum::extract::{Path, State};

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
