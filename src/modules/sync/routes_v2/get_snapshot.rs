use super::require_cloud::require_cloud;
use super::snapshot_query::SnapshotQuery;
use crate::app::AppState;

use crate::core::constants::modules;
use crate::core::error::{AppError, AppResult};

use crate::core::middleware::auth::CurrentUser;

use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::sync_v2::SnapshotResponse;
use crate::modules::sync::snapshot;

use axum::Json;
use axum::extract::{Query, State};

#[utoipa::path(get, path = "/snapshot", tag = modules::SYNC,
    params(SnapshotQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "A page of live rows as change records, plus asOfSeq to pull from afterwards", body = ApiResponse<SnapshotResponse>),
        (status = 400, description = "Invalid page token", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Not a device token", body = ErrorResponse),
        (status = 404, description = "Cloud sync is not enabled on this deployment", body = ErrorResponse),
        (status = 503, description = "APP_UPDATE_REQUIRED: the desktop app is too old to sync", body = ErrorResponse),
    )
)]
pub(super) async fn get_snapshot(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<SnapshotQuery>,
) -> AppResult<Json<ApiResponse<SnapshotResponse>>> {
    require_cloud(&state)?;
    let device_id = user.require_device()?;
    // Push, pull and register all refuse a revoked device; the snapshot (which
    // carries every user row, password hashes included) must too.
    if !super::super::cloud_store::ensure_device_active(&state.db, &device_id).await? {
        return Err(AppError::forbidden_with_code(
            "This device has been revoked",
            "DEVICE_REVOKED",
        ));
    }
    super::super::cloud_store::ensure_device_app_current(&state.db, &device_id).await?;
    let response = snapshot::build_snapshot(&state.db, query.page.as_deref(), query.limit).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Snapshot page retrieved",
    )))
}
