use crate::domain::purchase_receipts::ChargeKind;

/// The amount of a charge with its sign: a discount takes money off, everything else adds.
#[must_use]
pub fn signed_amount(kind: ChargeKind, amount_cents: i64) -> i64 {
    if kind == ChargeKind::Discount {
        -amount_cents
    } else {
        amount_cents
    }
}
