// Centralized monetary & financial calculations engine for jana2u-pos backend.
//
// Provides pure mathematical functions with 100% precision in integer cents:
// - Line item totals & discounts
// - Order-level discounts (percentage with half-up rounding matching frontend, fixed)
// - Sale subtotals, totals, and change due
// - Split payment validation

use crate::{
    core::error::{AppError, AppResult},
    domain::billing::{InvoiceItem, PricingAdjustments, SplitPayment},
};

/// Computes the net line total in cents from unit price, quantity, and discount.
#[inline]
pub fn compute_line_total(unit_price_cents: i64, quantity: i64, discount_cents: i64) -> i64 {
    let gross = unit_price_cents.max(0) * quantity.max(0);
    (gross - discount_cents.max(0)).max(0)
}

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
            // Standard half-up rounding: ((subtotal * pct / 100.0) + 0.5).floor()
            let raw_discount = (subtotal_cents as f64) * clamped_pct / 100.0;
            let discount_cents = (raw_discount.round() as i64).min(subtotal_cents).max(0);
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

/// Computes change due in cents if tendered amount was provided.
#[inline]
pub fn compute_change_due(amount_received_cents: Option<i64>, total_cents: i64) -> Option<i64> {
    amount_received_cents.map(|received| (received - total_cents).max(0))
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
        // 1140 * 0.10 = 114.0 -> 114
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
}
