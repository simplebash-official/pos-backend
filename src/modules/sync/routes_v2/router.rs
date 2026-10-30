use super::change_events;
use super::get_snapshot;
use super::mark_setup_complete;
use super::pull_changes;
use super::push_changes;
use super::register_device;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(push_changes::push_changes))
        .routes(routes!(pull_changes::pull_changes))
        .routes(routes!(change_events::change_events))
        .routes(routes!(get_snapshot::get_snapshot))
        .routes(routes!(register_device::register_device))
        .routes(routes!(mark_setup_complete::mark_setup_complete))
}
