use crate::domain::purchase_receipts::{ReceiptCharge, ReceiptSettlement};

/// What the service works out about a receipt from the records that point at it.
#[derive(Debug)]
pub struct ReceiptFigures {
    /// Taken off the bill by supplier credits.
    pub credited_cents: i64,
    /// The payments counted against the receipt, reversed ones included (marked as such).
    pub settlements: Vec<ReceiptSettlement>,
    /// The extra charges on the receipt.
    pub charges: Vec<ReceiptCharge>,
}
