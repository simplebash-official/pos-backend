use crate::core::error::{AppError, AppResult};

pub(super) fn validate_email(email: &str) -> AppResult<()> {
    let is_valid = email.matches('@').count() == 1
        && !email.contains(' ')
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        });

    if !is_valid {
        return Err(AppError::validation("Email is not a valid email address"));
    }
    Ok(())
}
