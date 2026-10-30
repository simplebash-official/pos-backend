use super::report_tz::report_tz;
use crate::core::error::{AppError, AppResult};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

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
            let end = match to.filter(|s| !s.trim().is_empty()) {
                Some(t) => parse_local_or_rfc3339(t.trim(), true, &local_midnight)?,
                None => local_midnight(today + Duration::days(1)),
            };
            let start = match from.filter(|s| !s.trim().is_empty()) {
                Some(f) => parse_local_or_rfc3339(f.trim(), false, &local_midnight)?,
                None => {
                    let default_start = local_midnight(today);
                    if end < default_start {
                        end - Duration::days(30)
                    } else {
                        default_start
                    }
                }
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
