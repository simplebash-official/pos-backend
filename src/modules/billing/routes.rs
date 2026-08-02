use axum::Json;
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{app::AppState, domain::ModuleStatusResponse};

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(status))
}

#[utoipa::path(get, path = "/", tag = "billing", responses(
    (status = 200, description = "Billing module status", body = ModuleStatusResponse)
))]
async fn status() -> Json<ModuleStatusResponse> {
    Json(ModuleStatusResponse {
        module: "billing".to_string(),
        status: "ok".to_string(),
    })
}
