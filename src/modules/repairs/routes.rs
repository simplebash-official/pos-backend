use axum::Json;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{app::AppState, core::response::ApiResponse, domain::ModuleStatusResponse};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(status))
}

#[utoipa::path(get, path = "/", tag = "repairs", responses(
    (status = 200, description = "Repairs module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    Json(ApiResponse::success(
        ModuleStatusResponse {
            module: "repairs".to_string(),
            status: "ok".to_string(),
        },
        "Repairs module status retrieved successfully",
    ))
}
