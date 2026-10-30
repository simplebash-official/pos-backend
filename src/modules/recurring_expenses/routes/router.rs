use super::create_template;
use super::get_period_status;
use super::list_bills;
use super::update_template;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_bills::list_bills))
        .routes(routes!(get_period_status::get_period_status))
        // Writes
        .routes(routes!(create_template::create_template))
        .routes(routes!(update_template::update_template))
}
