use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync_local::OutboxAckRequest;

use crate::modules::sync::state;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/outbox/ack", tag = modules::SYNC,
    request_body = OutboxAckRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Acknowledged outbox rows removed", body = ApiResponse<i64>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
pub(super) async fn ack_outbox(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Json(body): Json<OutboxAckRequest>,
) -> AppResult<Json<ApiResponse<i64>>> {
    let removed = state::ack_outbox(&app.db, body.up_to_seq).await?;
    Ok(Json(ApiResponse::success(removed, "Outbox acknowledged")))
}
