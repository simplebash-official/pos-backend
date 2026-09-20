// HTTP surface of cloud sync v2 (multi-tenant MongoDB deployments only):
// `POST /push`, `GET /pull`, `GET /snapshot` and `POST /devices/register`.
// All require a registered-device token (`CurrentUser::require_device`); a web
// owner session cannot call them - it uses the normal REST API - and a revoked
// device is refused with 403 `DEVICE_REVOKED`. On a single-shop deployment
// (SQLite desktop) they answer 404: those backends are the *clients* of this
// protocol, not its server.

use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        config::TenantMode,
        constants::modules,
        error::{AppError, AppResult},
        logging::domain::tracked,
        middleware::auth::CurrentUser,
        response::{ApiResponse, ErrorResponse},
    },
    domain::sync_v2::{PullResponse, PushRequest, PushResponse, SnapshotResponse},
    modules::sync::{cloud_store, pull, push, snapshot},
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(push_changes))
        .routes(routes!(pull_changes))
        .routes(routes!(get_snapshot))
        .routes(routes!(register_device))
}

// ============================================================================
// Helpers
// ============================================================================

fn require_cloud(state: &AppState) -> AppResult<()> {
    if state.config.tenant_mode == TenantMode::Multi {
        Ok(())
    } else {
        Err(AppError::not_found_with_code(
            "Cloud sync is not available on this deployment",
            "CLOUD_SYNC_DISABLED",
        ))
    }
}

// ============================================================================
// Push / pull / snapshot
// ============================================================================

#[utoipa::path(post, path = "/push", tag = modules::SYNC,
    request_body = PushRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Per-change acknowledgements (applied, duplicate, conflict, rejected)", body = ApiResponse<PushResponse>),
        (status = 400, description = "Batch too large or malformed", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Not a device token, device mismatch or revoked device", body = ErrorResponse),
        (status = 404, description = "Cloud sync is not enabled on this deployment", body = ErrorResponse),
    )
)]
async fn push_changes(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<PushRequest>,
) -> AppResult<Json<ApiResponse<PushResponse>>> {
    require_cloud(&state)?;
    let device_id = user.require_device()?;
    let response = tracked("sync.push", async {
        push::push(&state.db, &device_id, body).await
    })
    .await?;
    Ok(Json(ApiResponse::success(response, "Changes processed")))
}

/// Query of `GET /sync/pull`.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PullQuery {
    /// Last `seq` the device has applied (0 for a fresh device).
    pub since: Option<i64>,
    /// Page size (1..500, default 200).
    pub limit: Option<i64>,
}

#[utoipa::path(get, path = "/pull", tag = modules::SYNC,
    params(PullQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Changes after the cursor, oldest first", body = ApiResponse<PullResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Not a device token or revoked device", body = ErrorResponse),
        (status = 404, description = "Cloud sync is not enabled on this deployment", body = ErrorResponse),
        (status = 410, description = "CURSOR_EXPIRED: the history behind the cursor was compacted; bootstrap from a snapshot", body = ErrorResponse),
    )
)]
async fn pull_changes(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<PullQuery>,
) -> AppResult<Json<ApiResponse<PullResponse>>> {
    require_cloud(&state)?;
    let device_id = user.require_device()?;
    let response = pull::pull(&state.db, &device_id, query.since.unwrap_or(0), query.limit).await?;
    Ok(Json(ApiResponse::success(response, "Changes retrieved")))
}

/// Query of `GET /sync/snapshot`.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SnapshotQuery {
    /// Page token from the previous page (omit for the first page).
    pub page: Option<String>,
    /// Records per page (1..1000, default 500).
    pub limit: Option<i64>,
}

#[utoipa::path(get, path = "/snapshot", tag = modules::SYNC,
    params(SnapshotQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "A page of live rows as change records, plus asOfSeq to pull from afterwards", body = ApiResponse<SnapshotResponse>),
        (status = 400, description = "Invalid page token", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Not a device token", body = ErrorResponse),
        (status = 404, description = "Cloud sync is not enabled on this deployment", body = ErrorResponse),
    )
)]
async fn get_snapshot(
    user: CurrentUser,
    State(state): State<AppState>,
    Query(query): Query<SnapshotQuery>,
) -> AppResult<Json<ApiResponse<SnapshotResponse>>> {
    require_cloud(&state)?;
    user.require_device()?;
    let response = snapshot::snapshot(&state.db, query.page.as_deref(), query.limit).await?;
    Ok(Json(ApiResponse::success(response, "Snapshot page retrieved")))
}

// ============================================================================
// Device registration
// ============================================================================

/// Answer of `POST /sync/devices/register`.
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredDevice {
    pub device_id: String,
    /// Highest change `seq` the tenant has; a fresh device bootstraps to it.
    pub server_seq: i64,
}

#[utoipa::path(post, path = "/devices/register", tag = modules::SYNC,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "The device is registered for sync (idempotent)", body = ApiResponse<RegisteredDevice>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Not a device token or revoked device", body = ErrorResponse),
        (status = 404, description = "Cloud sync is not enabled on this deployment", body = ErrorResponse),
    )
)]
async fn register_device(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<RegisteredDevice>>> {
    require_cloud(&state)?;
    let device_id = user.require_device()?;
    if !cloud_store::ensure_device_active(&state.db, &device_id).await? {
        return Err(AppError::forbidden_with_code(
            "This device has been revoked",
            "DEVICE_REVOKED",
        ));
    }
    let server_seq = cloud_store::current_seq(&state.db).await?;
    Ok(Json(ApiResponse::success(
        RegisteredDevice {
            device_id,
            server_seq,
        },
        "Device registered",
    )))
}
