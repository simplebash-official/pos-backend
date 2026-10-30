use super::product_for::product_for;

use crate::app::AppState;
use crate::core::constants::{modules, permissions as perm};
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};

use crate::domain::inventory::{CreateProductRequest, Product};

use crate::modules::inventory::service;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;

#[utoipa::path(post, path = "/products", tag = modules::INVENTORY, request_body = CreateProductRequest,
    security(("bearerAuth" = [])),
    responses(
        (status = 201, description = "Product created", body = ApiResponse<Product>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 409, description = "SKU already exists", body = ErrorResponse),
    )
)]
pub(super) async fn create_product(
    user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateProductRequest>,
) -> AppResult<(StatusCode, Json<ApiResponse<Product>>)> {
    user.require_permission(perm::INVENTORY_WRITE)?;
    let product = product_for(
        &user,
        service::product::create_product(&state.db, body).await?,
    );

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(
            product,
            "Product created successfully",
        )),
    ))
}
