use super::analytics_payment_methods_response::AnalyticsPaymentMethodsResponse;
use super::analytics_summary_response::AnalyticsSummaryResponse;
use super::sales_patterns_response::SalesPatternsResponse;
use super::time_series_response::TimeSeriesResponse;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Overview section feed bundle.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OverviewFeedData {
    pub summary: AnalyticsSummaryResponse,
    pub timeseries: TimeSeriesResponse,
    pub payment_methods: AnalyticsPaymentMethodsResponse,
    pub sales_patterns: SalesPatternsResponse,
}
