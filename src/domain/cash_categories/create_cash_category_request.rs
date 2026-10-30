use super::report_group::ReportGroup;
use crate::domain::sessions::CashMovementType;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /cash-categories`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCashCategoryRequest {
    pub movement_type: CashMovementType,
    pub name: String,
    #[serde(default)]
    pub name_si: Option<String>,
    /// Only a manager or owner may choose; everyone else inherits it from the movement type.
    #[serde(default)]
    pub report_group: Option<ReportGroup>,
}
