use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync_local::ResolveConflictRequest;

use crate::modules::sync::state;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(post, path = "/conflicts/{key}/resolve", tag = modules::SYNC,
    params(("key" = String, Path, description = "Conflict key (scf_...)")),
    request_body = ResolveConflictRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Conflict marked resolved", body = ApiResponse<String>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
        (status = 404, description = "Conflict not found or already resolved", body = ErrorResponse),
    )
)]
pub(super) async fn resolve_conflict(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<ResolveConflictRequest>,
) -> AppResult<Json<ApiResponse<String>>> {
    state::resolve_conflict(&app.db, &key, &body.resolution).await?;
    Ok(Json(ApiResponse::success(key, "Conflict resolved")))
}
