use crate::core::error::{AppError, AppResult};
use crate::domain::billing::{InvoiceItem, PricingAdjustments, SplitPayment};
use pos_calc_core::calculations::percentage_discount_cents;

/// Computes order discount type, value, and cents against a subtotal using standard
/// half-up rounding for percentage discounts (matching frontend).
#[must_use]
pub fn compute_order_discount(
    subtotal_cents: i64,
    adjustments: Option<&PricingAdjustments>,
) -> (String, f64, i64) {
    if subtotal_cents <= 0 {
        return match adjustments {
            Some(adj) => (adj.discount_type.clone(), adj.discount_value, 0),
            None => ("fixed".to_string(), 0.0, 0),
        };
    }

    match adjustments {
        Some(PricingAdjustments {
            discount_type,
            discount_value,
        }) if discount_type == "percentage" => {
            let clamped_pct = discount_value.clamp(0.0, 100.0);
            let discount_cents = percentage_discount_cents(subtotal_cents, clamped_pct);
            (discount_type.clone(), clamped_pct, discount_cents)
        }
        Some(PricingAdjustments {
            discount_type,
            discount_value,
        }) if discount_type == "fixed" => {
            let fixed_cents = (*discount_value as i64).min(subtotal_cents).max(0);
            (discount_type.clone(), *discount_value, fixed_cents)
        }
        Some(PricingAdjustments {
            discount_type,
            discount_value,
        }) => (
            discount_type.clone(),
            *discount_value,
            (*discount_value as i64).min(subtotal_cents).max(0),
        ),
        None => ("fixed".to_string(), 0.0, 0),
    }
}

/// Computes all sale totals: (subtotal_cents, discount_type, discount_value, discount_cents, total_cents).
#[must_use]
pub fn compute_sale_totals(
    items: &[InvoiceItem],
    adjustments: Option<&PricingAdjustments>,
) -> (i64, String, f64, i64, i64) {
    let subtotal_cents: i64 = items.iter().map(|item| item.total_cents).sum();
    let (discount_type, discount_value, discount_cents) =
        compute_order_discount(subtotal_cents, adjustments);
    let total_cents = (subtotal_cents - discount_cents).max(0);
    (
        subtotal_cents,
        discount_type,
        discount_value,
        discount_cents,
        total_cents,
    )
}

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
