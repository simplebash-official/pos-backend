use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{AddSubcategoryRequest, CategoryInfo};

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

#[utoipa::path(post, path = "/categories/{categoryKey}/subcategories", tag = modules::INVENTORY,
    params(("categoryKey" = String, Path, description = "Category key")),
    request_body = AddSubcategoryRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Subcategory added", body = ApiResponse<CategoryInfo>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 404, description = "Category not found", body = ErrorResponse),
        (status = 409, description = "Subcategory already exists", body = ErrorResponse),
    )
)]
pub(super) async fn add_subcategory(
    _admin: AdminUser,
    State(state): State<AppState>,
    Path(category_key): Path<String>,
    Json(body): Json<AddSubcategoryRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CategoryInfo>>)> {
    let updated = service::category::add_subcategory(&state.db, category_key, body.name).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            updated,
            "Subcategory added successfully",
        )),
    ))
}
