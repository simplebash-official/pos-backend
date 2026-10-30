use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::sync_v2::{ApplyRequest, ApplyResponse};
use crate::modules::sync::apply;
use axum::Json;
use axum::extract::State;

#[utoipa::path(post, path = "/apply", tag = modules::SYNC,
    request_body = ApplyRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Batch applied in one transaction", body = ApiResponse<ApplyResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
pub(super) async fn apply_changes(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Json(body): Json<ApplyRequest>,
) -> AppResult<Json<ApiResponse<ApplyResponse>>> {
    let result = apply::apply_batch(&app.db, body).await?;
    // Sales, stock and balances just changed underneath the dashboards.
    app.reports_engine.invalidate_active();
    Ok(Json(ApiResponse::success(result, "Changes applied")))
}
