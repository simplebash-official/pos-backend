use crate::core::error::{AppError, AppResult};

/// A batch's prices must be usable: something to sell at, and a cost that
/// isn't negative. Same rule a product's prices followed before batches.
pub(crate) fn validate_batch_prices(
    unit_cost_cents: i64,
    selling_price_cents: i64,
) -> AppResult<()> {
    if selling_price_cents <= 0 {
        return Err(AppError::validation("Selling price must be greater than 0"));
    }
    if unit_cost_cents < 0 {
        return Err(AppError::validation("Unit cost cannot be negative"));
    }
    Ok(())
}
