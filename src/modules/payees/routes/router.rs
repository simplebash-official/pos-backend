use super::create_payee;
use super::list_payees;
use super::update_payee;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_payees::list_payees))
        .routes(routes!(create_payee::create_payee))
        .routes(routes!(update_payee::update_payee))
}
