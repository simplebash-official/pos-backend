use axum::Json;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{app::AppState, core::response::ApiResponse, domain::ModuleStatusResponse};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(status))
}

#[utoipa::path(get, path = "/", tag = "reports", responses(
    (status = 200, description = "Reports module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    Json(ApiResponse::success(
        ModuleStatusResponse {
            module: "reports".to_string(),
            status: "ok".to_string(),
        },
        "Reports module status retrieved successfully",
    ))
}
