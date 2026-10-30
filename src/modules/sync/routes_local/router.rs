use super::ack_outbox;
use super::apply_changes;
use super::enable_sync;
use super::get_outbox;
use super::get_pending;
use super::get_state;
use super::list_conflicts;
use super::local_events;
use super::resolve_conflict;
use super::seed_outbox;
use super::sku_prefixes;
use super::store_block;
use super::update_state;
use crate::app::AppState;

use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_state::get_state, update_state::update_state))
        .routes(routes!(enable_sync::enable_sync))
        .routes(routes!(get_outbox::get_outbox))
        .routes(routes!(local_events::local_events))
        .routes(routes!(get_pending::get_pending))
        .routes(routes!(ack_outbox::ack_outbox))
        .routes(routes!(seed_outbox::seed_outbox))
        .routes(routes!(apply_changes::apply_changes))
        .routes(routes!(store_block::store_block))
        .routes(routes!(sku_prefixes::sku_prefixes))
        .routes(routes!(list_conflicts::list_conflicts))
        .routes(routes!(resolve_conflict::resolve_conflict))
}
