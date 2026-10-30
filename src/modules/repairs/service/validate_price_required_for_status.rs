use crate::core::constants::codes;
use crate::core::error::{AppError, AppResult};

/// A ticket may sit priceless while `received`, `diagnosing`, `in_repair`
/// or `cancelled`; diagnosis-first repairs often can't be quoted before or
/// during repair work. Once it moves to `ready` or `delivered` a price must
/// exist, since those statuses mean the ticket is headed to pickup/billing.
/// Called on the *effective* post-write status/price so a single
/// PATCH that sets both fields together is validated against the combined
/// result.
const STATUSES_REQUIRING_PRICE: &[&str] = &["ready", "delivered"];

pub(super) fn validate_price_required_for_status(
    status: &str,
    estimated_cost_cents: Option<i64>,
) -> AppResult<()> {
    if STATUSES_REQUIRING_PRICE.contains(&status) && estimated_cost_cents.is_none() {
        return Err(AppError::validation_with_code(
            format!(
                "A repair price must be set before moving a ticket to '{status}'. Enter the repair price first."
            ),
            codes::REPAIR_PRICE_REQUIRED,
        ));
    }
    Ok(())
}
