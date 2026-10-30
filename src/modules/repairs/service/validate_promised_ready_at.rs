use crate::core::error::{AppError, AppResult};

/// A promised-ready date, when given, must be a real `YYYY-MM-DD` date.
pub(super) fn validate_promised_ready_at(promised_ready_at: Option<&str>) -> AppResult<()> {
    match promised_ready_at {
        None => Ok(()),
        Some(value) if chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() => Ok(()),
        Some(_) => Err(AppError::validation(
            "Promised ready date must be a valid date (YYYY-MM-DD)",
        )),
    }
}
