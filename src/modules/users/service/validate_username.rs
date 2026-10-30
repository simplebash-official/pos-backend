use crate::core::error::{AppError, AppResult};

/// Login usernames are lowercase `a-z0-9._-`, 3-32 characters, starting and
/// ending with a letter or digit. They are unique within a shop (the lookup is
/// tenant-scoped), so the same name (e.g. `admin`) can exist in many shops.
pub(super) fn validate_username(username: &str) -> AppResult<()> {
    let len = username.chars().count();
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c);
    let edge_ok = |c: Option<char>| c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if !(3..=32).contains(&len)
        || !username.chars().all(allowed)
        || !edge_ok(username.chars().next())
        || !edge_ok(username.chars().last())
    {
        return Err(AppError::validation(
            "Username must be 3-32 characters: lowercase letters, digits, '.', '_' or '-'",
        ));
    }
    Ok(())
}
