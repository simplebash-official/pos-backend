use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::repairs::{CreateRepairRequest, Repair};
use crate::modules::repairs::service;
use axum::Json;
use axum::extract::State;

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
