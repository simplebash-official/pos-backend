use crate::core::constants::modules;

use crate::core::response::ApiResponse;
use crate::core::utils::module_status_response;
use crate::domain::ModuleStatusResponse;

use axum::Json;

#[utoipa::path(get, path = "/", tag = modules::INVENTORY, responses(
    (status = 200, description = "Inventory module status", body = ApiResponse<ModuleStatusResponse>)
))]
pub(super) async fn status() -> Json<ApiResponse<ModuleStatusResponse>> {
    module_status_response(modules::INVENTORY)
}
