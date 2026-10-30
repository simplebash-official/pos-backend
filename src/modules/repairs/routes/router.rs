use super::create_repair;
use super::delete_repair;
use super::get_repair;
use super::get_repair_stats;
use super::list_repairs;
use super::update_repair;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(
            list_repairs::list_repairs,
            create_repair::create_repair
        ))
        .routes(routes!(get_repair_stats::get_repair_stats))
        .routes(routes!(
            get_repair::get_repair,
            update_repair::update_repair,
            delete_repair::delete_repair
        ))
}
