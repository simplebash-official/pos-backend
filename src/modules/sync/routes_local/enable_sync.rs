use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync_local::{EnableRequest, EnableResponse};

use crate::modules::sync::state;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/enable", tag = modules::SYNC,
    request_body(content = EnableRequest, description = "Cloud-issued device identity (optional)"),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Capture enabled and existing data enqueued", body = ApiResponse<EnableResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
pub(super) async fn enable_sync(
    _agent: SyncAgent,
    State(app): State<AppState>,
    body: Option<Json<EnableRequest>>,
) -> AppResult<Json<ApiResponse<EnableResponse>>> {
    let result = state::enable(&app.db, body.map(|Json(b)| b).unwrap_or_default()).await?;
    Ok(Json(ApiResponse::success(result, "Sync enabled")))
}
