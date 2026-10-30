use crate::core::error::{AppError, AppResult};

pub(super) fn validate_required_fields(
    customer_name: &str,
    job_type: &str,
    quantity: i32,
    estimated_cost_cents: i64,
) -> AppResult<()> {
    if customer_name.trim().is_empty() {
        return Err(AppError::validation("Customer name is required"));
    }
    if job_type.trim().is_empty() {
        return Err(AppError::validation("Job type is required"));
    }
    if quantity < 1 {
        return Err(AppError::validation("Quantity must be at least 1"));
    }
    if estimated_cost_cents < 0 {
        return Err(AppError::validation("Estimated cost cannot be negative"));
    }
    Ok(())
}
