use crate::core::error::{AppError, AppResult};
use chrono::NaiveDate;

/// A month (`YYYY-MM`) as its first day.
pub(super) fn parse_month(period: &str) -> AppResult<NaiveDate> {
    NaiveDate::parse_from_str(&format!("{period}-01"), "%Y-%m-%d")
        .map_err(|_| AppError::validation("Please pick a valid month."))
}
