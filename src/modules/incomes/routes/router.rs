use super::create_income;
use super::get_income;
use super::get_income_stats;
use super::list_incomes;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_incomes::list_incomes))
        .routes(routes!(get_income_stats::get_income_stats))
        .routes(routes!(get_income::get_income))
        // Writes
        .routes(routes!(create_income::create_income))
}
