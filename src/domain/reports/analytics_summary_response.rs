use super::analytics_kpi_deltas::AnalyticsKpiDeltas;
use super::analytics_kpis::AnalyticsKpis;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// `GET /reports/analytics/summary` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsSummaryResponse {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub current: AnalyticsKpis,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous: Option<AnalyticsKpis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deltas: Option<AnalyticsKpiDeltas>,
}
