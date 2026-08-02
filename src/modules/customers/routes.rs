use axum::Json;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{app::AppState, core::response::ApiResponse, domain::ModuleStatusResponse};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(status))
}

#[utoipa::path(get, path = "/", tag = "customers", responses(
    (status = 200, description = "Customers module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    Json(ApiResponse::success(
        ModuleStatusResponse {
            module: "customers".to_string(),
            status: "ok".to_string(),
        },
        "Customers module status retrieved successfully",
    ))
}
