use crate::core::error::{AppError, AppResult};

pub(super) fn validate_phone(phone: &str, field_label: &str) -> AppResult<()> {
    if !phone
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '+'))
    {
        return Err(AppError::validation(format!(
            "{field_label} may only contain digits, spaces, dashes, and a plus sign"
        )));
    }
    Ok(())
}
