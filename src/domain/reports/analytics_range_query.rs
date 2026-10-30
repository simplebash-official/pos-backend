use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

// Analytics & Reports — `/reports/analytics/*`
//
// Shop-local (Asia/Colombo, UTC+05:30) date handling via
// `service::dates::parse_date_range_tz`; time buckets via Mongo `$dateTrunc`.
// Every response is camelCase, money is integer cents. `*Bps` fields are
// basis points (1% = 100 bps) so ratios stay integer.

/// Shared query for the analytics endpoints. Presets match
/// `service::dates::parse_date_range` (`today` … `custom`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsRangeQuery {
    /// Preset range, or `"custom"` with `from`/`to`.
    pub preset: Option<String>,
    /// Start (YYYY-MM-DD in shop-local time, or RFC3339), used for `"custom"`.
    pub from: Option<String>,
    /// End (YYYY-MM-DD in shop-local time, or RFC3339), used for `"custom"`.
    pub to: Option<String>,
    /// Time-bucket size for series endpoints: `day` | `week` | `month` | `year`.
    /// Omitted → chosen automatically from the range span.
    pub granularity: Option<String>,
    /// When true, also compute the immediately-preceding equal-length window
    /// and per-metric deltas.
    pub compare_previous: Option<bool>,
}
