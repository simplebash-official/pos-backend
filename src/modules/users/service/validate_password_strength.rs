use crate::core::error::{AppError, AppResult};

pub(super) fn validate_password_strength(password: &str) -> AppResult<()> {
    if password.chars().count() < 8 {
        return Err(AppError::validation(
            "Password must be at least 8 characters",
        ));
    }
    Ok(())
}
