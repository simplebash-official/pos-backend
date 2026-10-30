use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::repairs::Repair;
use crate::modules::repairs::service;
use axum::Json;
use axum::extract::{Path, State};

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
