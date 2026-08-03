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

// Placeholder — replace with real print-queue handlers once this module
// is built out.
#[utoipa::path(get, path = "/", tag = modules::PRINT_JOBS, responses(
    (status = 200, description = "Print jobs module status", body = ApiResponse<ModuleStatusResponse>)
))]
async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::PRINT_JOBS)
}
