use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// An invoice's lifecycle state. There is deliberately no `Draft` variant —
/// this backend finalizes a sale as one atomic write (`complete_sale`),
/// there is no persisted pre-payment invoice for a "draft" to describe (the
/// cart / Held-Sales drawer plays that role client-side). `Cancelled` and
/// `Voided` are likewise collapsed into one action/status (`Voided`): stock
/// always decrements at sale time regardless of payment status, so there is
/// no zero-impact "called off before fulfillment" case to distinguish —
/// every void reverses stock/payment effects and carries an audit trail.
/// `Overdue` is deliberately not a variant either — it's a derived flag
/// (`Invoice.is_overdue`, computed at read time from `status`/`due_date`),
/// since a due date passing doesn't change what actually happened to the
/// invoice, just how it should be flagged in a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    /// Credit sale awaiting payment; nothing paid yet.
    Pending,
    /// A credit sale with some, but not all, of the total paid.
    PartiallyPaid,
    /// Fully paid — either at sale time or via installments.
    Paid,
    /// Invalidated after the fact — stock and payment effects reversed,
    /// with a mandatory reason and audit trail (`voided_at/by/reason`).
    Voided,
    /// Manually marked done: Paid, with no further activity expected.
    Closed,
}

impl InvoiceStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            InvoiceStatus::Pending => "pending",
            InvoiceStatus::PartiallyPaid => "partially_paid",
            InvoiceStatus::Paid => "paid",
            InvoiceStatus::Voided => "voided",
            InvoiceStatus::Closed => "closed",
        }
    }

    /// Maps "how much has been paid against this total" to the lifecycle
    /// state, so `complete_sale` (a deposit at checkout) and `record_payment`
    /// (later installments) derive the status the same way: nothing paid is
    /// `Pending`, the full amount (or more) is `Paid`, anything between is
    /// `PartiallyPaid`. Never returns `Voided`/`Closed` — those are set by
    /// their own explicit actions.
    #[must_use]
    pub fn from_payment_progress(total_cents: i64, paid_cents: i64) -> InvoiceStatus {
        if paid_cents >= total_cents {
            InvoiceStatus::Paid
        } else if paid_cents > 0 {
            InvoiceStatus::PartiallyPaid
        } else {
            InvoiceStatus::Pending
        }
    }
}
