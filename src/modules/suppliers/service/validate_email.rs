use crate::core::error::{AppError, AppResult};

/// A deliberately lightweight structural check (single `@`, non-empty local
/// part, dotted domain) rather than a full RFC 5322 validator — no `regex`
/// crate dependency exists in this codebase, and this is the same bar as
/// the codebase's other hand-rolled string validation (see
/// `core::utils::regex_escape`).
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
