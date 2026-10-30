use super::month_total::MonthTotal;

use serde::Serialize;
use utoipa::ToSchema;

/// One band of the monthly bar chart.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChartSeries {
    /// The category's name; empty for the "everything else" band, which the screen names.
    pub name: String,
    /// True for the band that adds up every category outside the biggest few.
    pub is_other: bool,
    /// Every month in the period, oldest first.
    pub months: Vec<MonthTotal>,
}
