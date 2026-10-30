use super::cash_flow;
use super::expense_breakdown;
use super::payables;
use super::profit_loss;
use crate::app::AppState;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Profit and costs
        .routes(routes!(profit_loss::profit_loss))
        .routes(routes!(expense_breakdown::expense_breakdown))
        // Money
        .routes(routes!(cash_flow::cash_flow))
        .routes(routes!(payables::payables))
}
