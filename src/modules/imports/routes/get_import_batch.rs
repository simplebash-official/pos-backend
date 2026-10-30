use crate::app::AppState;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::imports::ImportBatch;
use crate::modules::imports::service;
use axum::Json;
use axum::extract::{Path, State};

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
