use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::CategoryInfo;

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(delete, path = "/categories/{categoryKey}/subcategories/{subcategoryKey}", tag = modules::INVENTORY,
    params(
        ("categoryKey" = String, Path, description = "Category key"),
        ("subcategoryKey" = String, Path, description = "Subcategory key"),
    ),
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Subcategory removed", body = ApiResponse<CategoryInfo>),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Category or subcategory not found", body = ErrorResponse),
        (status = 409, description = "Subcategory is still referenced by products", body = ErrorResponse),
    )
)]
pub(super) async fn remove_subcategory(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path((category_key, subcategory_key)): Path<(String, String)>,
) -> AppResult<Json<ApiResponse<CategoryInfo>>> {
    let updated =
        service::category::remove_subcategory(&state.db, category_key, subcategory_key).await?;

    Ok(Json(ApiResponse::success(
        updated,
        "Subcategory removed successfully",
    )))
}
