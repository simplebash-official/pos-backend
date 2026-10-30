use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync_local::SyncStateResponse;

use crate::modules::sync::state;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/state", tag = modules::SYNC,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "This device's sync state", body = ApiResponse<SyncStateResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
pub(super) async fn get_state(
    _agent: SyncAgent,
    State(app): State<AppState>,
) -> AppResult<Json<ApiResponse<SyncStateResponse>>> {
    let result = state::get_state(&app.db).await?;
    Ok(Json(ApiResponse::success(result, "Sync state retrieved")))
}
