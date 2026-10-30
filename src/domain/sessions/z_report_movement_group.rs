use super::cash_direction::CashDirection;
use super::cash_movement_type::CashMovementType;

use serde::Serialize;
use utoipa::ToSchema;

/// What one shift took in or paid out under one category, for the Z-report. A reversed movement and its
/// reversal cancel out and are not counted.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ZReportMovementGroup {
    pub movement_type: CashMovementType,
    pub direction: CashDirection,
    pub category_name: String,
    pub count: u64,
    pub total_cents: i64,
}
