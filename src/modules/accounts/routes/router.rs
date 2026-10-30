use super::list_accounts;
use super::set_account_opening;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_accounts::list_accounts))
        // Writes
        .routes(routes!(set_account_opening::set_account_opening))
}
