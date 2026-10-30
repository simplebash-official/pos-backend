use super::register_device_request::RegisterDeviceRequest;
use super::registered_device::RegisteredDevice;
use super::require_cloud::require_cloud;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::{AppError, AppResult};

use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};

use crate::modules::sync::cloud_store;

use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/devices/register", tag = modules::SYNC,
    request_body = RegisterDeviceRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The device is registered for sync (idempotent)", body = ApiResponse<RegisteredDevice>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Not a device token or revoked device", body = ErrorResponse),
        (status = 404, description = "Cloud sync is not enabled on this deployment", body = ErrorResponse),
        (status = 503, description = "APP_UPDATE_REQUIRED: the desktop app is too old to sync", body = ErrorResponse),
    )
)]
pub(super) async fn register_device(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<RegisterDeviceRequest>,
) -> AppResult<Json<ApiResponse<RegisteredDevice>>> {
    require_cloud(&state)?;
    let device_id = user.require_device()?;
    if !cloud_store::ensure_device_active(&state.db, &device_id).await? {
        return Err(AppError::forbidden_with_code(
            "This device has been revoked",
            "DEVICE_REVOKED",
        ));
    }
    cloud_store::record_device_app_version(&state.db, &device_id, &body.app_version).await?;
    let server_seq = cloud_store::current_seq(&state.db).await?;
    Ok(Json(ApiResponse::success(
        RegisteredDevice {
            device_id,
            server_seq,
        },
        "Device registered",
    )))
}
