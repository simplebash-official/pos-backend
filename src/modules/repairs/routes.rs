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

#[utoipa::path(get, path = "/", tag = modules::REPAIRS, responses(
    (status = 200, description = "Repairs module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::REPAIRS)
}
