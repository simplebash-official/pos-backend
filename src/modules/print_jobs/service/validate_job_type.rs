use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

const VALID_JOB_TYPES: &[&str] = &["mug", "t-shirt", "handbill", "banner", "custom"];

pub(super) fn validate_job_type(job_type: &str) -> AppResult<()> {
    if VALID_JOB_TYPES.contains(&job_type) {
        Ok(())
    } else {
        Err(AppError::validation_with_code(
            format!(
                "Invalid job type '{job_type}'. Must be one of: {}",
                VALID_JOB_TYPES.join(", ")
            ),
            codes::INVALID_JOB_TYPE,
        ))
    }
}
