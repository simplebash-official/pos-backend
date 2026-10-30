use crate::core::error::{AppError, AppResult};

/// Name/phone/commission-split invariants shared by create and update.
pub(super) fn validate_required_fields(
    name: &str,
    phone: &str,
    default_split_value: f64,
) -> AppResult<()> {
    if name.trim().chars().count() < 2 {
        return Err(AppError::validation(
            "Employee name must be at least 2 characters",
        ));
    }
    if phone.trim().is_empty() {
        return Err(AppError::validation("Phone is required"));
    }
    if !phone
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '+'))
    {
        return Err(AppError::validation(
            "Phone may only contain digits, spaces, dashes, and a plus sign",
        ));
    }
    if default_split_value < 0.0 {
        return Err(AppError::validation(
            "Default commission split value cannot be negative",
        ));
    }
    Ok(())
}
