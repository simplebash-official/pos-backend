use super::get_cash_settings;
use super::update_cash_settings;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(get_cash_settings::get_cash_settings))
        // Writes
        .routes(routes!(update_cash_settings::update_cash_settings))
}
