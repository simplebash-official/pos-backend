use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync_local::StoreBlockRequest;

use crate::modules::sync::blocks;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/blocks", tag = modules::SYNC,
    request_body = StoreBlockRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Cloud-reserved number block stored", body = ApiResponse<String>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
pub(super) async fn store_block(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Json(body): Json<StoreBlockRequest>,
) -> AppResult<Json<ApiResponse<String>>> {
    blocks::store_block(&app.db, body).await?;
    Ok(Json(ApiResponse::success(
        "stored".to_string(),
        "Number block stored",
    )))
}
