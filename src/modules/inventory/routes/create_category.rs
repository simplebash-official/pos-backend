use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{CategoryInfo, CreateCategoryRequest};

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(post, path = "/categories", tag = modules::INVENTORY, request_body = CreateCategoryRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Category created", body = ApiResponse<CategoryInfo>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 403, description = "Admin access required", body = ErrorResponse),
        (status = 409, description = "Category already exists", body = ErrorResponse),
    )
)]
pub(super) async fn create_category(
    _admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<CreateCategoryRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<CategoryInfo>>)> {
    let category = service::category::create_category(&state.db, body).await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            category,
            "Category created successfully",
        )),
    ))
}
