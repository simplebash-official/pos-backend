use crate::core::error::{AppError, AppResult};

/// A tenant id must be one the identity service issued: `tnt_...`.
pub(super) fn validate_tenant_key(key: &str) -> AppResult<()> {
    if !key.starts_with("tnt_") || key.len() < 8 || key.chars().any(char::is_whitespace) {
        return Err(AppError::validation(
            "Tenant id must look like tnt_... (copy it from the identity service)",
        ));
    }
    Ok(())
}
