// Date calculation and parsing utilities for reporting periods and presets.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};

use crate::core::error::{AppError, AppResult};

/// Computes the `[start, end)` UTC bounds for a given preset or custom date range.
pub(crate) fn parse_date_range(
    preset: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<(DateTime<Utc>, DateTime<Utc>)> {
    let now = Utc::now();
    let today = now.date_naive();
    let today_start = today
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is valid")
        .and_utc();

    match preset.map(|p| p.to_lowercase()).as_deref() {
        Some("today") => {
            let end = today_start + Duration::days(1);
            Ok((today_start, end))
        }
        Some("yesterday") => {
            let start = today_start - Duration::days(1);
            Ok((start, today_start))
        }
        Some("this_week") => {
            let days_since_monday = today.weekday().num_days_from_monday() as i64;
            let start = (today - Duration::days(days_since_monday))
                .and_hms_opt(0, 0, 0)
                .expect("valid time")
                .and_utc();
            let end = start + Duration::days(7);
            Ok((start, end))
        }
        Some("this_month") => {
            let year = today.year();
            let month = today.month();
            let start = NaiveDate::from_ymd_opt(year, month, 1)
                .expect("valid 1st of month")
                .and_hms_opt(0, 0, 0)
                .expect("valid time")
                .and_utc();

            let next_month_date = if month == 12 {
                NaiveDate::from_ymd_opt(year + 1, 1, 1).expect("valid Jan 1 next year")
            } else {
                NaiveDate::from_ymd_opt(year, month + 1, 1).expect("valid next month")
            };
            let end = next_month_date
                .and_hms_opt(0, 0, 0)
                .expect("valid time")
                .and_utc();
            Ok((start, end))
        }
        Some("last_month") => {
            let year = today.year();
            let month = today.month();
            let (last_year, last_month) = if month == 1 {
                (year - 1, 12)
            } else {
                (year, month - 1)
            };
            let start = NaiveDate::from_ymd_opt(last_year, last_month, 1)
                .expect("valid 1st of last month")
                .and_hms_opt(0, 0, 0)
                .expect("valid time")
                .and_utc();
            let end = NaiveDate::from_ymd_opt(year, month, 1)
                .expect("valid 1st of this month")
                .and_hms_opt(0, 0, 0)
                .expect("valid time")
                .and_utc();
            Ok((start, end))
        }
        Some("this_year") => {
            let year = today.year();
            let start = NaiveDate::from_ymd_opt(year, 1, 1)
                .expect("valid Jan 1")
                .and_hms_opt(0, 0, 0)
                .expect("valid time")
                .and_utc();
            let end = NaiveDate::from_ymd_opt(year + 1, 1, 1)
                .expect("valid Jan 1 next year")
                .and_hms_opt(0, 0, 0)
                .expect("valid time")
                .and_utc();
            Ok((start, end))
        }
        Some("all_time") => {
            let start = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
            let end = today_start + Duration::days(1);
            Ok((start, end))
        }
        Some("custom") | None => {
            let start = if let Some(f) = from.filter(|s| !s.trim().is_empty()) {
                parse_iso_or_ymd(f.trim(), false)?
            } else {
                today_start
            };

            let end = if let Some(t) = to.filter(|s| !s.trim().is_empty()) {
                parse_iso_or_ymd(t.trim(), true)?
            } else {
                today_start + Duration::days(1)
            };

            if start > end {
                return Err(AppError::validation("Start date cannot be after end date"));
            }

            Ok((start, end))
        }
        Some(unknown) => Err(AppError::validation(format!(
            "Unknown date preset '{unknown}'. Supported presets: today, yesterday, this_week, this_month, last_month, this_year, all_time, custom"
        ))),
    }
}

/// Parses a date string as either YYYY-MM-DD or RFC3339.
/// For YYYY-MM-DD, if `is_end_bound` is true, returns next day at 00:00:00 UTC (exclusive upper bound).
pub(crate) fn parse_iso_or_ymd(s: &str, is_end_bound: bool) -> AppResult<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }

    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let target_date = if is_end_bound {
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
