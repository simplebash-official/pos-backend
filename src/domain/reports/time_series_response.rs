use super::time_series_point::TimeSeriesPoint;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// `GET /reports/analytics/timeseries` response.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeSeriesResponse {
    /// Resolved granularity (`day` | `week` | `month` | `year`).
    pub granularity: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    /// Continuous, gap-filled buckets in ascending order.
    pub points: Vec<TimeSeriesPoint>,
    /// Sum across all buckets.
    pub totals: TimeSeriesPoint,
}
