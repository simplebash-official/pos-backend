use super::get_changes;
use super::sync_status;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_changes::get_changes))
        .routes(routes!(sync_status::sync_status))
        .merge(crate::modules::sync::routes_v2::router())
        .merge(crate::modules::sync::routes_local::router())
}
