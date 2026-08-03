use axum::Json;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    app::AppState,
    core::{constants::modules, response::ApiResponse, utils::module_status_response},
    domain::ModuleStatusResponse,
};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(status))
}

// Placeholder — replace with real login/refresh handlers that issue JWTs
// verified by `core::middleware::auth`.
#[utoipa::path(get, path = "/", tag = modules::AUTH, responses(
    (status = 200, description = "Auth module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::AUTH)
}
