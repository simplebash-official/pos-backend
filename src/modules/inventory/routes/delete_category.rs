use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::CategoryInfo;

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(delete, path = "/categories/{categoryKey}", tag = modules::INVENTORY,
    params(("categoryKey" = String, Path, description = "Category key")),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Category deleted", body = ApiResponse<CategoryInfo>),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Category not found", body = ErrorResponse),
        (status = 409, description = "Category is still referenced by products", body = ErrorResponse),
    )
)]
pub(super) async fn delete_category(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(category_key): Path<String>,
) -> AppResult<Json<ApiResponse<CategoryInfo>>> {
    let deleted = service::category::delete_category(&state.db, category_key).await?;

    Ok(Json(ApiResponse::success(
        deleted,
        "Category deleted successfully",
    )))
}
