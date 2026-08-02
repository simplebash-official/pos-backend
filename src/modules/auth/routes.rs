use axum::Json;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{app::AppState, domain::ModuleStatusResponse};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(status))
}

// Placeholder — replace with real login/refresh handlers that issue JWTs
// verified by `core::middleware::auth`.
#[utoipa::path(get, path = "/", tag = "auth", responses(
    (status = 200, description = "Auth module status", body = ModuleStatusResponse)
))]
async fn status() -> Json<ModuleStatusResponse> {
    Json(ModuleStatusResponse {
        module: "auth".to_string(),
        status: "ok".to_string(),
    })
}
