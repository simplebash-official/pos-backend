use crate::core::constants::codes;
use crate::core::error::AppError;

/// `AppError` for a batch key that names no live batch (of the expected product).
pub(crate) fn batch_not_found(key: &str) -> AppError {
    AppError::not_found_with_code(
        format!("Stock batch '{key}' not found"),
        codes::STOCK_BATCH_NOT_FOUND,
    )
}
