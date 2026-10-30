use crate::core::error::AppError;

const JOB_NOT_FOUND: &str = "LABEL_JOB_NOT_FOUND";

pub(super) fn job_not_found() -> AppError {
    AppError::not_found_with_code("Label print not found", JOB_NOT_FOUND)
}
