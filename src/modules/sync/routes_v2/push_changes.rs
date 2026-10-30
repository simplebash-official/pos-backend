use super::require_cloud::require_cloud;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::logging::domain::tracked;
use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::sync_v2::{PushRequest, PushResponse};
use crate::modules::sync::push;

use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/push", tag = modules::SYNC,
    request_body = PushRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Per-change acknowledgements (applied, duplicate, conflict, rejected)", body = ApiResponse<PushResponse>),
        (status = 400, description = "Batch too large or malformed", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Not a device token, device mismatch or revoked device", body = ErrorResponse),
        (status = 404, description = "Cloud sync is not enabled on this deployment", body = ErrorResponse),
        (status = 503, description = "APP_UPDATE_REQUIRED: the desktop app is too old to sync", body = ErrorResponse),
    )
)]
pub(super) async fn push_changes(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<PushRequest>,
) -> AppResult<Json<ApiResponse<PushResponse>>> {
    require_cloud(&state)?;
    let device_id = user.require_device()?;
    let response = tracked("sync.push", async {
        push::push_batch(&state.db, &device_id, body).await
    })
    .await?;
    // Pushed sales, payments and returns change today's figures just like the
    // web's own billing writes do.
    state.reports_engine.invalidate_active();
    Ok(Json(ApiResponse::success(response, "Changes processed")))
}
