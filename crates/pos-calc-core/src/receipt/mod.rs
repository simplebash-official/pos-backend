//! Receipt (stock intake) calculation: lines, charges, payment and the blockers that stop a save.

pub mod blocker;
pub mod charges;
mod compute;
pub mod modes;
pub mod payment_source;
pub mod pricing;
pub mod reply;
pub mod request;

pub use blocker::{Blocker, BlockerCode};
pub use charges::{Allocation, BilledBy, ChargeFigure, ChargeKind, ChargeTotals};
pub use compute::{CHARGE_DESCRIPTION_MAX, REFERENCE_MAX, compute};
pub use modes::{CostMode, PaymentMode, PricingMode};
pub use payment_source::PaymentSource;
pub use pricing::LineMargin;
pub use reply::{
    CalcLineReply, CalcMargin, CalcReceiptLineReply, CalcReceiptReply, CalcReceiptSummary,
};
pub use request::{CalcCharge, CalcChargesPayment, CalcLine, CalcPayment, CalcReceiptRequest};
