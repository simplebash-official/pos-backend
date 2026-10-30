use super::validate_phone::validate_phone;

use crate::core::error::{AppError, AppResult};

use crate::domain::suppliers::ContactPerson;

/// Invariants for contact persons: at least one contact person is required,
/// each person must have a non-empty name, between 1 and 2 telephone numbers (maximum 2),
/// and all telephone numbers must contain valid phone characters.
pub(super) fn validate_contact_persons(contact_persons: &[ContactPerson]) -> AppResult<()> {
    if contact_persons.is_empty() {
        return Err(AppError::validation(
            "At least one contact person is required",
        ));
    }
    for (idx, cp) in contact_persons.iter().enumerate() {
        let num = idx + 1;
        if cp.name.trim().is_empty() {
            return Err(AppError::validation(format!(
                "Contact person {num} name is required"
            )));
        }
        if cp.phone_numbers.is_empty() {
            return Err(AppError::validation(format!(
                "Contact person '{}' must have at least one phone number",
                cp.name.trim()
            )));
        }
        if cp.phone_numbers.len() > 2 {
            return Err(AppError::validation(format!(
                "Contact person '{}' can have at most 2 phone numbers",
                cp.name.trim()
            )));
        }
        for (p_idx, phone) in cp.phone_numbers.iter().enumerate() {
            let label = if p_idx == 0 {
                format!("Phone number for {}", cp.name.trim())
            } else {
                format!("Secondary phone for {}", cp.name.trim())
            };
            if phone.trim().is_empty() {
                return Err(AppError::validation(format!("{label} cannot be empty")));
            }
            validate_phone(phone.trim(), &label)?;
        }
    }
    Ok(())
}
