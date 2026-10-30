use crate::core::error::{AppError, AppResult};

use argon2::Argon2;

use argon2::password_hash::{PasswordHash, PasswordVerifier};

pub(super) fn verify_password(password: &str, hash: &str) -> AppResult<bool> {
    let parsed_hash = PasswordHash::new(hash)
        .map_err(|err| AppError::internal(format!("stored password hash is invalid: {err}")))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}
