use crate::core::error::{AppError, AppResult};
use chrono::NaiveDate;

pub(super) fn parse_day(value: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| AppError::validation("Please pick a valid date."))
}
