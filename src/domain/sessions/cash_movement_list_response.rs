use super::cash_movement::CashMovement;

use serde::Serialize;
use utoipa::ToSchema;

/// Paginated list of cash movements.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CashMovementListResponse {
    pub movements: Vec<CashMovement>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}
