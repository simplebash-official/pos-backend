use super::sanitize_string::sanitize_string;
use super::validate_contact_persons::validate_contact_persons;

use crate::core::error::{AppError, AppResult};

use crate::domain::suppliers::ContactPerson;

pub(super) fn resolve_and_validate_contacts(
    contact_persons: Option<Vec<ContactPerson>>,
    legacy_contact: Option<String>,
    legacy_primary: Option<String>,
    legacy_secondary: Option<String>,
) -> AppResult<(Vec<ContactPerson>, String, String, Option<String>)> {
    let resolved = if let Some(cps) = contact_persons {
        cps
    } else if let (Some(cp), Some(pp)) = (legacy_contact, legacy_primary) {
        if cp.trim().is_empty() || pp.trim().is_empty() {
            return Err(AppError::validation(
                "Contact person and primary phone are required",
            ));
        }
        let mut phones = vec![pp.trim().to_string()];
        if let Some(sec) = legacy_secondary.and_then(|s| sanitize_string(Some(s))) {
            phones.push(sec);
        }
        vec![ContactPerson {
            name: cp.trim().to_string(),
            phone_numbers: phones,
        }]
    } else {
        return Err(AppError::validation(
            "At least one contact person is required",
        ));
    };

    validate_contact_persons(&resolved)?;

    let primary_cp = &resolved[0];
    let contact_person = primary_cp.name.trim().to_string();
    let primary_phone = primary_cp.phone_numbers[0].trim().to_string();
    let secondary_phone = primary_cp
        .phone_numbers
        .get(1)
        .map(|p| p.trim().to_string());

    Ok((resolved, contact_person, primary_phone, secondary_phone))
}
