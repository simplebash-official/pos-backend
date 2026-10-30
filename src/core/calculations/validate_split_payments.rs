use crate::core::error::{AppError, AppResult};
use crate::domain::billing::SplitPayment;

/// Validates split payment legs against total due.
pub fn validate_split_payments(total_cents: i64, legs: &[SplitPayment]) -> AppResult<()> {
    if legs.is_empty() {
        return Err(AppError::validation(
            "A split payment must include at least one split leg",
        ));
    }

    for leg in legs {
        if leg.amount_cents < 0 {
            return Err(AppError::validation(
                "Split payment leg amount cannot be negative",
            ));
        }
    }

    let legs_total: i64 = legs.iter().map(|leg| leg.amount_cents).sum();
    if legs_total > total_cents {
        return Err(AppError::validation(
            "Split payment legs cannot sum to more than the invoice total",
        ));
    }

    Ok(())
}
