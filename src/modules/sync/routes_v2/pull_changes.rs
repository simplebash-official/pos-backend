use super::pull_query::PullQuery;
use super::require_cloud::require_cloud;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::AppResult;

use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::sync_v2::PullResponse;
use crate::modules::sync::pull;

use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/pull", tag = modules::SYNC,
    params(PullQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Changes after the cursor, oldest first", body = ApiResponse<PullResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Not a device token or revoked device", body = ErrorResponse),
        (status = 404, description = "Cloud sync is not enabled on this deployment", body = ErrorResponse),
        (status = 410, description = "CURSOR_EXPIRED: the history behind the cursor was compacted; bootstrap from a snapshot", body = ErrorResponse),
        (status = 503, description = "APP_UPDATE_REQUIRED: the desktop app is too old to sync", body = ErrorResponse),
    )
)]
pub(super) async fn pull_changes(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<PullQuery>,
) -> AppResult<Json<ApiResponse<PullResponse>>> {
    require_cloud(&state)?;
    let device_id = user.require_device()?;
    let response =
        pull::pull_since(&state.db, &device_id, query.since.unwrap_or(0), query.limit).await?;
    Ok(Json(ApiResponse::success(response, "Changes retrieved")))
}
