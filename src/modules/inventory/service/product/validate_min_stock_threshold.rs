use crate::core::error::{AppError, AppResult};

pub(super) fn validate_min_stock_threshold(min_stock_threshold: i64) -> AppResult<()> {
    if min_stock_threshold < 0 {
        return Err(AppError::validation(
            "Minimum stock threshold cannot be negative",
        ));
    }
    Ok(())
}
