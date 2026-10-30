use super::add_expense_payment;
use super::create_expense;
use super::get_expense;
use super::get_expense_stats;
use super::list_expenses;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Reads
        .routes(routes!(list_expenses::list_expenses))
        .routes(routes!(get_expense_stats::get_expense_stats))
        .routes(routes!(get_expense::get_expense))
        // Writes
        .routes(routes!(create_expense::create_expense))
        .routes(routes!(add_expense_payment::add_expense_payment))
}
