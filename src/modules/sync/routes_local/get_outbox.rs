use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync_local::{OutboxQuery, OutboxResponse};

use crate::modules::sync::outbox;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/outbox", tag = modules::SYNC, params(OutboxQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Queued local changes as canonical records", body = ApiResponse<OutboxResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
pub(super) async fn get_outbox(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Query(query): Query<OutboxQuery>,
) -> AppResult<Json<ApiResponse<OutboxResponse>>> {
    let result = outbox::list_outbox(&app.db, query).await?;
    Ok(Json(ApiResponse::success(result, "Outbox retrieved")))
}
