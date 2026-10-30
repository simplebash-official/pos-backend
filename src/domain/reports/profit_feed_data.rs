use super::monthly_profit_report_response::MonthlyProfitReportResponse;
use super::time_series_response::TimeSeriesResponse;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Profit section feed bundle.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfitFeedData {
    pub monthly_profit: MonthlyProfitReportResponse,
    pub timeseries: TimeSeriesResponse,
}
