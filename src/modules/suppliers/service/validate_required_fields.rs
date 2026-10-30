use crate::core::error::{AppError, AppResult};

/// Name/contact-person/phone/address/category invariants shared by create,
/// full replace, and (on the merged result) partial update — kept separate
/// from `validate_email` since the latter only applies to a field that's
/// genuinely optional on the entity, not one that's always present.
pub(super) fn validate_required_fields(
    name: &str,
    contact_person: &str,
    primary_phone: &str,
    address: &str,
    supplied_categories: &[String],
) -> AppResult<()> {
    if name.trim().chars().count() < 2 {
        return Err(AppError::validation(
            "Supplier name must be at least 2 characters",
        ));
    }
    if contact_person.trim().is_empty() {
        return Err(AppError::validation("Contact person is required"));
    }
    if primary_phone.trim().is_empty() {
        return Err(AppError::validation("Primary phone is required"));
    }
    if !primary_phone
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ' ' | '-' | '+'))
    {
        return Err(AppError::validation(
            "Primary phone may only contain digits, spaces, dashes, and a plus sign",
        ));
    }
    if address.trim().is_empty() {
        return Err(AppError::validation("Address is required"));
    }
    if supplied_categories.is_empty() {
        return Err(AppError::validation(
            "At least one supplied category is required",
        ));
    }
    Ok(())
}
