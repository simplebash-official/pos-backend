use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::SyncAgent;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::sync_local::SkuPrefixesResponse;

use crate::modules::sync::blocks;
use axum::Json;
use axum::extract::State;

#[utoipa::path(get, path = "/sku-prefixes", tag = modules::SYNC,
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "SKU block families this device's local catalog currently needs", body = ApiResponse<SkuPrefixesResponse>),
        (status = 401, description = "Missing or invalid service token", body = ErrorResponse),
    )
)]
pub(super) async fn sku_prefixes(
    _agent: SyncAgent,
    State(app): State<AppState>,
) -> AppResult<Json<ApiResponse<SkuPrefixesResponse>>> {
    let prefixes = blocks::local_sku_prefixes(&app.db).await?;
    Ok(Json(ApiResponse::success(
        SkuPrefixesResponse { prefixes },
        "SKU prefixes listed",
    )))
}
