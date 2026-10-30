use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::middleware::auth::CurrentUser;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::settings::ShopProfileResponse;
use crate::modules::settings::service;
use axum::Json;

use axum::extract::State;

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
