//! Centralized monetary & financial calculations engine for simplebash-pos backend.
//! The pure calculations live in `pos_calc_core::calculations`; this module re-exports them
//! and provides domain-specific bindings for `InvoiceItem`, `PricingAdjustments`, and `SplitPayment`.

pub use pos_calc_core::calculations::{
    commission_cents, compute_change_due, compute_line_total, default_refund_legs,
    invoice_paid_and_balance, job_profit_cents, refund_amounts, returned_unit_price_cents,
    share_basis_points,
};
mod core_calculations;
#[cfg(test)]
mod tests;

pub use core_calculations::compute_order_discount;
pub use core_calculations::compute_sale_totals;
pub use core_calculations::validate_split_payments;
