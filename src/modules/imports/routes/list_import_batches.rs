use crate::app::AppState;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::imports::{ImportBatchListQuery, ImportBatchListResponse};
use crate::modules::imports::service;
use axum::Json;
use axum::extract::{Query, State};

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
