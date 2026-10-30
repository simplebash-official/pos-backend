use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Where a receipt stands, worked out from what is still due.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    /// Nothing paid.
    Unpaid,
    /// Some paid, some still due.
    PartlyPaid,
    /// Nothing left to pay.
    Paid,
}

impl ReceiptStatus {
    /// `due` is what is left after credits and payments; `paid_or_credited` is what has been
    /// taken off the bill either way.
    #[must_use]
    pub fn of(due_cents: i64, paid_or_credited_cents: i64) -> Self {
        if due_cents <= 0 {
            ReceiptStatus::Paid
        } else if paid_or_credited_cents > 0 {
            ReceiptStatus::PartlyPaid
        } else {
            ReceiptStatus::Unpaid
        }
    }
}
