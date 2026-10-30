use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync_local::{PendingQuery, PendingResponse};

use crate::modules::sync::outbox;
use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/outbox/pending", tag = modules::SYNC, params(PendingQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Changes still waiting to upload, with readable names", body = ApiResponse<PendingResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
pub(super) async fn get_pending(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Query(query): Query<PendingQuery>,
) -> AppResult<Json<ApiResponse<PendingResponse>>> {
    let result = outbox::list_pending(&app.db, query).await?;
    Ok(Json(ApiResponse::success(
        result,
        "Pending changes retrieved",
    )))
}
