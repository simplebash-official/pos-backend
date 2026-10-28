// Centralized monetary & financial calculations engine for simplebash-pos backend.
// The pure calculations live in `pos_calc_core::calculations`; this module re-exports them
// and provides domain-specific bindings for `InvoiceItem`, `PricingAdjustments`, and `SplitPayment`.

use crate::{
    core::error::{AppError, AppResult},
    domain::billing::{InvoiceItem, PricingAdjustments, SplitPayment},
};

pub use pos_calc_core::calculations::*;

/// Computes order discount type, value, and cents against a subtotal using standard
/// half-up rounding for percentage discounts (matching frontend).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_line_total() {
        assert_eq!(compute_line_total(490, 2, 0), 980);
        assert_eq!(compute_line_total(650, 1, 65), 585);
        assert_eq!(compute_line_total(500, 1, 800), 0);
    }

    #[test]
    fn test_compute_order_discount_percentage_half_up() {
        let adj = PricingAdjustments {
            discount_type: "percentage".to_string(),
            discount_value: 10.0,
        };
        let (dtype, dval, dcents) = compute_order_discount(1140, Some(&adj));
        assert_eq!(dtype, "percentage");
        assert_eq!(dval, 10.0);
        assert_eq!(dcents, 114);
    }

    #[test]
    fn test_compute_order_discount_fixed() {
        let adj = PricingAdjustments {
            discount_type: "fixed".to_string(),
            discount_value: 140.0,
        };
        let (_, _, dcents) = compute_order_discount(1140, Some(&adj));
        assert_eq!(dcents, 140);
    }

    #[test]
    fn test_compute_sale_totals() {
        let items = vec![
            InvoiceItem {
                product_key: None,
                name: "Item 1".to_string(),
                sku: None,
                unit_price_cents: 490,
                quantity: 1,
                discount_cents: 0,
                total_cents: 490,
                unit_cost_cents: None,
                source_type: "retail".to_string(),
                source_ticket_key: None,
                source_ticket_number: None,
                assigned_employee_name: None,
                returned_quantity: 0,
                serial_numbers: vec![],
                batch_allocations: vec![],
            },
            InvoiceItem {
                product_key: None,
                name: "Item 2".to_string(),
                sku: None,
                unit_price_cents: 650,
                quantity: 1,
                discount_cents: 0,
                total_cents: 650,
                unit_cost_cents: None,
                source_type: "retail".to_string(),
                source_ticket_key: None,
                source_ticket_number: None,
                assigned_employee_name: None,
                returned_quantity: 0,
                serial_numbers: vec![],
                batch_allocations: vec![],
            },
        ];

        let (subtotal, _, _, discount, total) = compute_sale_totals(&items, None);
        assert_eq!(subtotal, 1140);
        assert_eq!(discount, 0);
        assert_eq!(total, 1140);
    }

    #[test]
    fn test_compute_change_due() {
        assert_eq!(compute_change_due(Some(2000), 1140), Some(860));
        assert_eq!(compute_change_due(Some(1000), 1140), Some(0));
        assert_eq!(compute_change_due(None, 1140), None);
    }

    #[test]
    fn test_validate_split_payments() {
        let legs = vec![
            SplitPayment {
                method: "cash".to_string(),
                amount_cents: 500,
                card_last4: None,
                reference: None,
            },
            SplitPayment {
                method: "card".to_string(),
                amount_cents: 500,
                card_last4: None,
                reference: None,
            },
        ];
        assert!(validate_split_payments(1000, &legs).is_ok());
        assert!(validate_split_payments(900, &legs).is_err());
        assert!(validate_split_payments(1000, &[]).is_err());
    }
}
