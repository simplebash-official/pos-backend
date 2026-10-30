use crate::app::AppState;
use crate::core::constants::modules;
use crate::core::error::AppResult;
use crate::core::response::{ApiResponse, ErrorResponse};
use crate::domain::auth::ShopLookupResponse;
use crate::modules::auth::service;
use axum::Json;
use axum::extract::{Path, State};

#[utoipa::path(get, path = "/shop/{code}", tag = modules::AUTH,
    params(
        ("code" = String, Path, description = "Shop code to look up")
    ),
    responses(
        (status = 200, description = "Shop details retrieved successfully", body = ApiResponse<ShopLookupResponse>),
        (status = 404, description = "Shop not found", body = ErrorResponse),
    )
)]
pub(super) async fn lookup_shop(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> AppResult<Json<ApiResponse<ShopLookupResponse>>> {
    let response = service::lookup_shop(&state.db, &code).await?;
    Ok(Json(ApiResponse::success(
        response,
        "Shop details retrieved successfully",
    )))
}
