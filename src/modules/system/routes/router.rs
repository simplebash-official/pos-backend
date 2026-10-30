use super::get_installation;
use super::get_setup_status;
use super::perform_setup;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_setup_status::get_setup_status))
        .routes(routes!(perform_setup::perform_setup))
        .routes(routes!(get_installation::get_installation))
}
