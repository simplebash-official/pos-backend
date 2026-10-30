use super::normalize_username::normalize_username;

use crate::core::constants::roles;
use crate::core::error::{AppError, AppResult};

use argon2::password_hash::PasswordHash;

/// Checks an owner login handed over by the identity service without writing
/// anything, so a provisioning request can be rejected before any tenant row is
/// created. The username must be the fixed Admin username, and the hash must be
/// an Argon2 PHC string: anything else could never be verified by
/// `verify_password` and would create an Admin nobody can log in as.
pub fn validate_owner_login(name: &str, username: &str, password_hash: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::validation("Name is required"));
    }
    if normalize_username(username) != roles::ADMIN_USERNAME {
        return Err(AppError::validation(format!(
            "The owner username must be '{}'",
            roles::ADMIN_USERNAME
        )));
    }
    let not_argon2 = || AppError::validation("Owner password hash must be an Argon2 PHC string");
    let parsed = PasswordHash::new(password_hash).map_err(|_| not_argon2())?;
    if !parsed.algorithm.as_str().starts_with("argon2") {
        return Err(not_argon2());
    }
    Ok(())
}
