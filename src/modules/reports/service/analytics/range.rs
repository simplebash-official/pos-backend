use super::super::dates;

use crate::core::error::AppResult;
use crate::domain::reports::AnalyticsRangeQuery;

use chrono::{DateTime, Utc};

/// Resolve `[start, end)` from an `AnalyticsRangeQuery` (shop-local).
pub(super) fn range(q: &AnalyticsRangeQuery) -> AppResult<(DateTime<Utc>, DateTime<Utc>)> {
    dates::parse_date_range_tz(q.preset.as_deref(), q.from.as_deref(), q.to.as_deref())
}
