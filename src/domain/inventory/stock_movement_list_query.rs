use super::stock_movement_type::StockMovementType;

use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /inventory/stock-movements`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct StockMovementListQuery {
    /// Filter movements by product ID.
    pub product_id: Option<String>,
    /// Filter movements by movement type.
    pub movement_type: Option<StockMovementType>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page limit.
    pub limit: Option<u64>,
}
