use crate::core::error::AppError;

pub(super) fn job_err(action: &str, e: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("Failed to {action} label print job: {e}"))
}
