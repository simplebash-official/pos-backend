use crate::core::error::{AppError, AppResult};

/// Lowercases and validates a shop code: 3-32 chars of `a-z`, `0-9` and `-`,
/// not starting or ending with `-`.
pub fn normalize_shop_code(raw: &str) -> AppResult<String> {
    let code = raw.trim().to_ascii_lowercase();
    let valid_chars = code
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if code.len() < 3
        || code.len() > 32
        || !valid_chars
        || code.starts_with('-')
        || code.ends_with('-')
    {
        return Err(AppError::validation(
            "Shop code must be 3-32 characters: lowercase letters, digits and hyphens, not starting or ending with a hyphen",
        ));
    }
    Ok(code)
}
