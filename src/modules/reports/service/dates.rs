// Date calculation and parsing utilities for reporting periods and presets.

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Utc};

use crate::core::error::{AppError, AppResult};

/// Shop-local timezone for reporting. Sri Lanka is permanently UTC+05:30 — no
/// DST since 2006 — so a fixed offset is exact and needs no `chrono-tz`
/// dependency. The new analytics pipelines bucket and label by this local
/// time (via `parse_date_range_tz` for the `$match` bounds and the
/// `REPORT_TZ_MONGO` string passed to `$dateTrunc` / `$hour` / `$isoDayOfWeek`)
/// so "Tuesday", "August", and "9am" mean what the shop owner expects; a
/// bare-UTC bucket pushes the local 00:00–05:29 slice into the previous day.
/// The older endpoints (`dashboard`, `daily-sales`, `monthly-profit`, …) stay
/// on the UTC `parse_date_range` below — they are almost always "today" /
/// "this_month" where the boundary barely moves.
pub(crate) const REPORT_TZ_OFFSET_SECS: i32 = 5 * 3600 + 30 * 60;

/// The same offset as a string MongoDB's date operators accept in their
/// `timezone` field.
pub(crate) const REPORT_TZ_MONGO: &str = "+05:30";

pub(crate) fn report_tz() -> FixedOffset {
    FixedOffset::east_opt(REPORT_TZ_OFFSET_SECS).expect("valid fixed offset")
}

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
            let y = today.year();
            let start = NaiveDate::from_ymd_opt(y - 1, 1, 1)
                .expect("valid Jan 1 last year")
                .and_hms_opt(0, 0, 0)
                .expect("valid time")
                .and_utc();
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

/// The shop-local (`report_tz`) equivalent of `parse_date_range`: preset and
/// `YYYY-MM-DD` bounds are resolved against Sri Lanka local time, then
/// converted to the `[start, end)` UTC window the Mongo `$match` needs. An
/// RFC3339 `from`/`to` is already absolute and passes through unchanged.
/// Used by every `/reports/analytics/*` endpoint; the older endpoints keep
/// `parse_date_range` (UTC).
pub(crate) fn parse_date_range_tz(
    preset: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
) -> AppResult<(DateTime<Utc>, DateTime<Utc>)> {
    let tz = report_tz();
    let now_local = Utc::now().with_timezone(&tz);
    let today = now_local.date_naive();

    // Local midnight of `date`, as an absolute UTC instant.
    let local_midnight = |date: NaiveDate| -> DateTime<Utc> {
        date.and_hms_opt(0, 0, 0)
            .expect("00:00:00 is valid")
            .and_local_timezone(tz)
            .single()
            .expect("fixed offset has no ambiguous local times")
            .with_timezone(&Utc)
    };

    let first_of_month = |year: i32, month: u32| -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, 1).expect("valid 1st of month")
    };

    match preset.map(|p| p.to_lowercase()).as_deref() {
        Some("today") => Ok((
            local_midnight(today),
            local_midnight(today + Duration::days(1)),
        )),
        Some("yesterday") => Ok((
            local_midnight(today - Duration::days(1)),
            local_midnight(today),
        )),
        Some("this_week") => {
            let days_since_monday = today.weekday().num_days_from_monday() as i64;
            let start_date = today - Duration::days(days_since_monday);
            Ok((
                local_midnight(start_date),
                local_midnight(start_date + Duration::days(7)),
            ))
        }
        Some("this_month") => {
            let (y, m) = (today.year(), today.month());
            let next = if m == 12 {
                first_of_month(y + 1, 1)
            } else {
                first_of_month(y, m + 1)
            };
            Ok((local_midnight(first_of_month(y, m)), local_midnight(next)))
        }
        Some("last_month") => {
            let (y, m) = (today.year(), today.month());
            let (ly, lm) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
            Ok((
                local_midnight(first_of_month(ly, lm)),
                local_midnight(first_of_month(y, m)),
            ))
        }
        Some("this_year") => {
            let y = today.year();
            Ok((
                local_midnight(first_of_month(y, 1)),
                local_midnight(first_of_month(y + 1, 1)),
            ))
        }
        Some("all_time") => {
            let y = today.year();
            let start = first_of_month(y - 1, 1);
            Ok((
                local_midnight(start),
                local_midnight(today + Duration::days(1)),
            ))
        }
        Some("custom") | None => {
            let start = match from.filter(|s| !s.trim().is_empty()) {
                Some(f) => parse_local_or_rfc3339(f.trim(), false, &local_midnight)?,
                None => local_midnight(today),
            };
            let end = match to.filter(|s| !s.trim().is_empty()) {
                Some(t) => parse_local_or_rfc3339(t.trim(), true, &local_midnight)?,
                None => local_midnight(today + Duration::days(1)),
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

fn parse_local_or_rfc3339(
    s: &str,
    is_end_bound: bool,
    local_midnight: &dyn Fn(NaiveDate) -> DateTime<Utc>,
) -> AppResult<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let date = if is_end_bound {
            d + Duration::days(1)
        } else {
            d
        };
        return Ok(local_midnight(date));
    }
    Err(AppError::validation(format!(
        "Invalid date format '{s}'. Expected YYYY-MM-DD or RFC3339 timestamp."
    )))
}

#[cfg(test)]
mod tz_tests {
    use super::*;

    #[test]
    fn today_bounds_are_local_midnight_in_utc() {
        let (start, end) = parse_date_range_tz(Some("today"), None, None).unwrap();
        // 24h window, and start is 18:30 UTC of the previous day (00:00 +05:30).
        assert_eq!(end - start, Duration::days(1));
        assert_eq!(start.format("%H:%M").to_string(), "18:30");
    }

    #[test]
    fn custom_ymd_is_interpreted_in_shop_local_time() {
        let (start, end) =
            parse_date_range_tz(Some("custom"), Some("2026-08-01"), Some("2026-08-31")).unwrap();
        assert_eq!(start.to_rfc3339(), "2026-07-31T18:30:00+00:00");
        // end bound is exclusive: local midnight of 1 Sep.
        assert_eq!(end.to_rfc3339(), "2026-08-31T18:30:00+00:00");
    }
}
