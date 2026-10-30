use super::list_sessions;
use super::login;
use super::lookup_shop;
use super::me;
use super::set_pin;
use super::update_my_preferences;
use super::verify_pin;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(login::login))
        .routes(routes!(lookup_shop::lookup_shop))
        .routes(routes!(me::me))
        .routes(routes!(update_my_preferences::update_my_preferences))
        .routes(routes!(list_sessions::list_sessions))
        .routes(routes!(verify_pin::verify_pin))
        .routes(routes!(set_pin::set_pin))
}
