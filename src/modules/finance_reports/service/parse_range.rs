use crate::core::error::{AppError, AppResult};
use chrono::NaiveDate;

/// The longest range a report covers, so one request cannot read years of records.
const MAX_DAYS: i64 = 732;

fn parse_day(label: &str, value: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::validation(format!("Please pick a valid {label}.")))
}

/// Checks a `from`..`to` range of days and returns it as dates.
pub(super) fn parse_range(from: &str, to: &str) -> AppResult<(NaiveDate, NaiveDate)> {
    let (start, end) = (parse_day("start date", from)?, parse_day("end date", to)?);
    if end < start {
        return Err(AppError::validation(
            "The end date cannot be before the start date.",
        ));
    }
    if (end - start).num_days() > MAX_DAYS {
        return Err(AppError::validation(
            "Please pick a period of at most two years.",
        ));
    }
    Ok((start, end))
}
