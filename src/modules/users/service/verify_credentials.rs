use super::hash_password::hash_password;
use super::normalize_username::normalize_username;
use super::verify_password::verify_password;
use crate::clients::db::Db;
use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

use crate::domain::users::User;

use crate::modules::users::repository;

/// A real Argon2 hash of a throwaway value, computed once, for
/// `verify_credentials` to verify against when the username is unknown.
static DUMMY_PASSWORD_HASH: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    hash_password("not-a-real-password").expect("hashing a constant cannot fail")
});

/// Cross-module entry point `auth::service::login` calls. "No such username"
/// and "wrong password" both collapse to the same 401
/// `INVALID_CREDENTIALS` (never distinguish which — avoids username
/// enumeration); a correct password against a deactivated account gets its
/// own 401 `USER_INACTIVE`, since at that point the caller has already
/// proven they know the real password.
pub(crate) async fn verify_credentials(db: &Db, username: &str, password: &str) -> AppResult<User> {
    let normalized = normalize_username(username);
    let Some(document) = repository::find_user_by_username(db, &normalized).await? else {
        // Spend the same Argon2 work as a real check, so response time
        // doesn't reveal whether the username exists.
        let _ = verify_password(password, &DUMMY_PASSWORD_HASH);
        return Err(AppError::unauthorized_with_code(
            "Invalid username or password",
            codes::INVALID_CREDENTIALS,
        ));
    };

    if !verify_password(password, &document.password_hash)? {
        return Err(AppError::unauthorized_with_code(
            "Invalid username or password",
            codes::INVALID_CREDENTIALS,
        ));
    }

    if !document.is_active {
        return Err(AppError::unauthorized_with_code(
            "This account has been deactivated",
            codes::USER_INACTIVE,
        ));
    }

    Ok(document.into_user())
}
