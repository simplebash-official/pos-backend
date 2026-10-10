// HTTP route handlers and OpenAPI declarations for settings and shop profiles.

use axum::{Json, extract::State};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{
        constants::modules,
        error::AppResult,
        middleware::auth::{AdminUser, CurrentUser},
        response::{ApiResponse, ErrorResponse},
    },
    domain::settings::{ShopProfileResponse, UpdateShopProfileRequest},
    modules::settings::service,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_shop_profile, update_shop_profile))
}

/// Retrieve the active shop profile.
#[utoipa::path(
    get,
    path = "/shop-profile",
    tag = modules::SETTINGS,
    responses(
        (status = 200, description = "Shop profile retrieved successfully", body = ApiResponse<ShopProfileResponse>),
        (status = 401, description = "Missing or invalid token", body = ErrorResponse),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_shop_profile(
    user: CurrentUser,
    State(state): State<AppState>,
) -> AppResult<Json<ApiResponse<ShopProfileResponse>>> {
    let profile = service::get_shop_profile(&state.db, user.tenant_id.as_deref()).await?;
    Ok(Json(ApiResponse::success(
        profile,
        "Shop profile retrieved successfully",
    )))
}

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
