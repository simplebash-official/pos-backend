use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::AdminUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::settings::{ShopProfileResponse, UpdateShopProfileRequest};
use crate::modules::settings::service;
use axum::Json;

use axum::extract::State;

/// Update the active shop profile.
#[utoipa::path(
    put,
    path = "/shop-profile",
    tag = modules::SETTINGS,
    request_body = UpdateShopProfileRequest,
    responses(
        (status = 200, description = "Shop profile updated successfully", body = ApiResponse<ShopProfileResponse>),
        (status = 400, description = "Validation error", body = ErrorResponse),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
        (status = 403, description = "Admin role required", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn update_shop_profile(
    admin: AdminUser,
    State(state): State<AppState>,
    Json(body): Json<UpdateShopProfileRequest>,
) -> AppResult<Json<ApiResponse<ShopProfileResponse>>> {
    let profile =
        service::update_shop_profile(&state.db, admin.0.tenant_id.as_deref(), body).await?;
    Ok(Json(ApiResponse::success(
        profile,
        "Shop profile updated successfully",
    )))
}
