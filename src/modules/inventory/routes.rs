use axum::Json;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{app::AppState, domain::ModuleStatusResponse};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(status))
}

#[utoipa::path(get, path = "/", tag = "inventory", responses(
    (status = 200, description = "Inventory module status", body = ModuleStatusResponse)
))]
async fn status() -> Json<ModuleStatusResponse> {
    Json(ModuleStatusResponse {
        module: "inventory".to_string(),
        status: "ok".to_string(),
    })
}
