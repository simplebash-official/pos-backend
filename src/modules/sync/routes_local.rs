// Device-local sync API, called only by the desktop shell's sync agent with a
// service token (`SyncAgent`). It exposes this install's sync state, its
// outbox (what to push), the applier (what was pulled), cloud-reserved number
// blocks and the reviewable conflict list. These routes exist only on SQLite
// (desktop) deployments; on the cloud they answer 404.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        middleware::auth::SyncAgent,
        response::{ApiResponse, ErrorResponse},
    },
    domain::{
        sync_local::{
            ConflictItem, EnableRequest, EnableResponse, OutboxAckRequest, OutboxQuery,
            OutboxResponse, ResolveConflictRequest, SeedResponse, StoreBlockRequest,
            SyncStateResponse, UpdateSyncStateRequest,
        },
        sync_v2::{ApplyRequest, ApplyResponse},
    },
    modules::sync::{apply, blocks, outbox, state},
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_state, update_state))
        .routes(routes!(enable_sync))
        .routes(routes!(get_outbox))
        .routes(routes!(ack_outbox))
        .routes(routes!(seed_outbox))
        .routes(routes!(apply_changes))
        .routes(routes!(store_block))
        .routes(routes!(list_conflicts))
        .routes(routes!(resolve_conflict))
}

// ============================================================================
// State
// ============================================================================

#[utoipa::path(get, path = "/state", tag = modules::SYNC,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "This device's sync state", body = ApiResponse<SyncStateResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
async fn get_state(
    _agent: SyncAgent,
    State(app): State<AppState>,
) -> AppResult<Json<ApiResponse<SyncStateResponse>>> {
    let result = state::get_state(&app.db).await?;
    Ok(Json(ApiResponse::success(result, "Sync state retrieved")))
}

#[utoipa::path(post, path = "/state", tag = modules::SYNC,
    request_body = UpdateSyncStateRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Updated sync state", body = ApiResponse<SyncStateResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
async fn update_state(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Json(body): Json<UpdateSyncStateRequest>,
) -> AppResult<Json<ApiResponse<SyncStateResponse>>> {
    let result = state::update_state(&app.db, body).await?;
    Ok(Json(ApiResponse::success(result, "Sync state updated")))
}

#[utoipa::path(post, path = "/enable", tag = modules::SYNC,
    request_body(content = EnableRequest, description = "Cloud-issued device identity (optional)"),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Capture enabled and existing data enqueued", body = ApiResponse<EnableResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
async fn enable_sync(
    _agent: SyncAgent,
    State(app): State<AppState>,
    body: Option<Json<EnableRequest>>,
) -> AppResult<Json<ApiResponse<EnableResponse>>> {
    let result = state::enable(&app.db, body.map(|Json(b)| b).unwrap_or_default()).await?;
    Ok(Json(ApiResponse::success(result, "Sync enabled")))
}

// ============================================================================
// Outbox
// ============================================================================

#[utoipa::path(get, path = "/outbox", tag = modules::SYNC, params(OutboxQuery),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Queued local changes as canonical records", body = ApiResponse<OutboxResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
async fn get_outbox(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Query(query): Query<OutboxQuery>,
) -> AppResult<Json<ApiResponse<OutboxResponse>>> {
    let result = outbox::list_outbox(&app.db, query).await?;
    Ok(Json(ApiResponse::success(result, "Outbox retrieved")))
}

#[utoipa::path(post, path = "/outbox/ack", tag = modules::SYNC,
    request_body = OutboxAckRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Acknowledged outbox rows removed", body = ApiResponse<i64>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
async fn ack_outbox(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Json(body): Json<OutboxAckRequest>,
) -> AppResult<Json<ApiResponse<i64>>> {
    let removed = state::ack_outbox(&app.db, body.up_to_seq).await?;
    Ok(Json(ApiResponse::success(removed, "Outbox acknowledged")))
}

#[utoipa::path(post, path = "/outbox/seed", tag = modules::SYNC,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Every existing row enqueued", body = ApiResponse<SeedResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
async fn seed_outbox(
    _agent: SyncAgent,
    State(app): State<AppState>,
) -> AppResult<Json<ApiResponse<SeedResponse>>> {
    let result = state::seed_outbox(&app.db).await?;
    Ok(Json(ApiResponse::success(result, "Outbox seeded")))
}

// ============================================================================
// Apply
// ============================================================================

#[utoipa::path(post, path = "/apply", tag = modules::SYNC,
    request_body = ApplyRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Batch applied in one transaction", body = ApiResponse<ApplyResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
async fn apply_changes(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Json(body): Json<ApplyRequest>,
) -> AppResult<Json<ApiResponse<ApplyResponse>>> {
    let result = apply::apply_batch(&app.db, body).await?;
    // Sales, stock and balances just changed underneath the dashboards.
    app.reports_engine.invalidate_active();
    Ok(Json(ApiResponse::success(result, "Changes applied")))
}

// ============================================================================
// Number blocks
// ============================================================================

#[utoipa::path(post, path = "/blocks", tag = modules::SYNC,
    request_body = StoreBlockRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Cloud-reserved number block stored", body = ApiResponse<String>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
async fn store_block(
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

// ============================================================================
// Conflicts
// ============================================================================

#[utoipa::path(get, path = "/conflicts", tag = modules::SYNC,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Unresolved sync conflicts", body = ApiResponse<Vec<ConflictItem>>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
async fn list_conflicts(
    _agent: SyncAgent,
    State(app): State<AppState>,
) -> AppResult<Json<ApiResponse<Vec<ConflictItem>>>> {
    let items = state::list_conflicts(&app.db).await?;
    Ok(Json(ApiResponse::success(items, "Conflicts retrieved")))
}

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
async fn resolve_conflict(
    _agent: SyncAgent,
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<ResolveConflictRequest>,
) -> AppResult<Json<ApiResponse<String>>> {
    state::resolve_conflict(&app.db, &key, &body.resolution).await?;
    Ok(Json(ApiResponse::success(
        key,
        "Conflict resolved",
    )))
}
