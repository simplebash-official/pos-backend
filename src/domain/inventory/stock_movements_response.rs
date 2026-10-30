use super::stock_movement::StockMovement;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /products/{id}/movements`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct StockMovementsResponse {
    /// List of historical stock movements for a specific product.
    pub movements: Vec<StockMovement>,
}
