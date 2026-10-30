use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync_local::SeedResponse;

use crate::modules::sync::state;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/outbox/seed", tag = modules::SYNC,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Every existing row enqueued", body = ApiResponse<SeedResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
pub(super) async fn seed_outbox(
    _agent: SyncAgent,
    State(app): State<AppState>,
) -> AppResult<Json<ApiResponse<SeedResponse>>> {
    let result = state::seed_outbox(&app.db).await?;
    Ok(Json(ApiResponse::success(result, "Outbox seeded")))
}
