use crate::core::error::AppError;

pub(super) fn store_err(action: &str, e: impl std::fmt::Display) -> AppError {
    AppError::internal(format!("Failed to {action} label template: {e}"))
}
