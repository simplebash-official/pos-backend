use crate::app::AppState;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::ApiResponse;
use crate::domain::imports::{ImportBatch, ProcessImportRequest};
use crate::modules::imports::service;
use axum::Json;
use axum::extract::State;

// (Module-specific helpers if needed)

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
