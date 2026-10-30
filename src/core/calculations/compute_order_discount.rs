use crate::domain::billing::PricingAdjustments;
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
