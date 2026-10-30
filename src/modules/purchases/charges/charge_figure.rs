use crate::domain::purchase_receipts::{BilledBy, ChargeKind};

/// One extra charge reduced to what the owed-total needs.
#[derive(Debug, Clone, Copy)]
pub struct ChargeFigure {
    pub kind: ChargeKind,
    pub amount_cents: i64,
    pub billed_by: BilledBy,
}
