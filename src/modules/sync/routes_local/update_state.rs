use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync_local::{SyncStateResponse, UpdateSyncStateRequest};

use crate::modules::sync::state;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/state", tag = modules::SYNC,
    request_body = UpdateSyncStateRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Updated sync state", body = ApiResponse<SyncStateResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
pub(super) async fn update_state(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Json(body): Json<UpdateSyncStateRequest>,
) -> AppResult<Json<ApiResponse<SyncStateResponse>>> {
    let result = state::update_state(&app.db, body).await?;
    Ok(Json(ApiResponse::success(result, "Sync state updated")))
}
