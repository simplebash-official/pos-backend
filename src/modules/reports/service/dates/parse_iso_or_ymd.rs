use super::range_bound::RangeBound;
use crate::core::error::{AppError, AppResult};
use chrono::{DateTime, Duration, NaiveDate, Utc};

/// Parses a date string as either YYYY-MM-DD or RFC3339.
/// For YYYY-MM-DD, `RangeBound::End` returns the next day at 00:00:00 UTC (exclusive upper bound).
pub(crate) fn parse_iso_or_ymd(s: &str, bound: RangeBound) -> AppResult<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }

    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let target_date = if bound == RangeBound::End {
            d + Duration::days(1)
        } else {
            d
        };
        return Ok(target_date
            .and_hms_opt(0, 0, 0)
            .expect("00:00:00 is valid")
            .and_utc());
    }

    Err(AppError::validation(format!(
        "Invalid date format '{s}'. Expected YYYY-MM-DD or RFC3339 timestamp."
    )))
}
