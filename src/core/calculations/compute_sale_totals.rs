use super::compute_order_discount::compute_order_discount;

use crate::domain::billing::{InvoiceItem, PricingAdjustments};

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
