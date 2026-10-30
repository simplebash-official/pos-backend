use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

const VALID_STATUSES: &[&str] = &["received", "in_progress", "ready", "delivered", "cancelled"];

pub(super) fn validate_status(status: &str) -> AppResult<()> {
    if VALID_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(AppError::validation_with_code(
            format!(
                "Invalid status '{status}'. Must be one of: {}",
                VALID_STATUSES.join(", ")
            ),
            codes::INVALID_JOB_STATUS,
        ))
    }
}
