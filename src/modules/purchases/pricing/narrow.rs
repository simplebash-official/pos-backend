use crate::core::error::{AppError, AppResult};

pub(super) fn narrow(value: i128) -> AppResult<i64> {
    i64::try_from(value).map_err(|_| AppError::validation("That amount is too large"))
}
