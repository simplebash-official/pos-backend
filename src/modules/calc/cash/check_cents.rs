use crate::core::error::{AppError, AppResult};

const MAX_CENTS: i64 = 10_000_000_000_000;

pub(super) fn check_cents(label: &str, cents: i64) -> AppResult<()> {
    if !(0..=MAX_CENTS).contains(&cents) {
        return Err(AppError::validation(format!("{label} is out of range")));
    }
    Ok(())
}
