// HTTP route handlers and OpenAPI declarations for batch imports.

use axum::{
    Json,
    extract::{Path, Query, State},
};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{error::AppResult, middleware::auth::CurrentUser, response::ApiResponse},
    domain::imports::{
        ImportBatch, ImportBatchListQuery, ImportBatchListResponse, ProcessImportRequest,
    },
    modules::imports::service,
};

// ============================================================================
// Router
// ============================================================================

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(process_import, list_import_batches))
        .routes(routes!(get_import_batch))
}

// ============================================================================
// Helpers
// ============================================================================

// (Module-specific helpers if needed)

// ============================================================================
// Imports Handlers
// ============================================================================

/// Process a batch data import from extracted spreadsheet rows.
#[utoipa::path(
    post,
    path = "",
    tag = "imports",
    request_body = ProcessImportRequest,
    responses(
        (status = 200, description = "Batch import processed successfully", body = ApiResponse<ImportBatch>),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Permission denied")
    ),
    security(("bearerAuth" = []))
)]
pub async fn process_import(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ProcessImportRequest>,
) -> AppResult<Json<ApiResponse<ImportBatch>>> {
    let result = service::batch::process_import(&state.db, &user, body).await?;
    Ok(Json(ApiResponse::success(
        result,
        "Import batch processed successfully",
    )))
}

/// List previously executed batch imports with pagination.
#[utoipa::path(
    get,
    path = "",
    tag = "imports",
    params(ImportBatchListQuery),
    responses(
        (status = 200, description = "List of import batches", body = ApiResponse<ImportBatchListResponse>),
        (status = 401, description = "Unauthorized")
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_import_batches(
    State(state): State<AppState>,
    _user: CurrentUser,
    Query(query): Query<ImportBatchListQuery>,
) -> AppResult<Json<ApiResponse<ImportBatchListResponse>>> {
    let response = service::batch::list_import_batches(&state.db, query).await?;
    Ok(Json(ApiResponse::data(response)))
}

/// Retrieve full details and row error records for a specific import batch.
#[utoipa::path(
    get,
    path = "/{key}",
    tag = "imports",
    params(
        ("key" = String, Path, description = "Unique import batch business key (e.g. imp_...)")
    ),
    responses(
        (status = 200, description = "Import batch details", body = ApiResponse<ImportBatch>),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Import batch not found")
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_import_batch(
    State(state): State<AppState>,
    _user: CurrentUser,
    Path(key): Path<String>,
) -> AppResult<Json<ApiResponse<ImportBatch>>> {
    let result = service::batch::get_import_batch(&state.db, &key).await?;
    Ok(Json(ApiResponse::data(result)))
}
