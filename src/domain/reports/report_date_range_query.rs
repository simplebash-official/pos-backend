use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

/// Query parameters for date-filtered reports with preset or custom range.
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ReportDateRangeQuery {
    /// Preset range: `"today"`, `"yesterday"`, `"this_week"`, `"this_month"`,
    /// `"last_month"`, `"this_year"`, `"all_time"`, or `"custom"`.
    pub preset: Option<String>,
    /// Start date (YYYY-MM-DD or ISO 8601 timestamp), used for `"custom"`.
    pub from: Option<String>,
    /// End date (YYYY-MM-DD or ISO 8601 timestamp), used for `"custom"`.
    pub to: Option<String>,
}
