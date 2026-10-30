use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Request body for recording an additional payment against an existing invoice.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordPaymentRequest {
    /// Repayment amount in cents.
    pub amount_cents: i64,
    /// Payment method used ("cash", "card", "online").
    pub payment_method: String,
    /// Optional reference note or description.
    #[serde(default)]
    pub notes: Option<String>,
    /// Register session key if taken during an open cashier shift.
    #[serde(default)]
    pub session_key: Option<String>,
}

/// One method/amount leg of a credit note's cash refund payout — plural
/// because a credit note against a split-paid invoice must refund across
/// the same methods (for till/card-settlement reconciliation), not as one
/// lump sum.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RefundBreakdownLeg {
    /// Payment method this leg refunds ("cash", "card", "online").
    pub method: String,
    /// Amount refunded via this method in cents.
    pub amount_cents: i64,
}

/// Reason why an item was returned by a customer — independent of its
/// physical `ItemCondition`; a customer can return a `Resalable` item for
/// `WarrantyClaim`, or a `Damaged` one for `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReturnReason {
    #[serde(alias = "defective")]
    Defective,
    #[serde(alias = "wrong_item", alias = "wrongItem")]
    WrongItem,
    #[serde(alias = "customer_changed_mind", alias = "customerChangedMind")]
    CustomerChangedMind,
    #[serde(alias = "warranty_claim", alias = "warrantyClaim")]
    WarrantyClaim,
    #[serde(alias = "other")]
    Other,
}

/// One leg of a split payment. `method` is `"cash"` | `"card"` | `"online"`
/// (never `"split"` — that's the invoice-level `payment_method` value that
/// means "see `split_payments`").
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SplitPayment {
    /// Payment method used for this portion ("cash", "card", "online").
    pub method: String,
    /// Amount paid via this payment method in cents.
    pub amount_cents: i64,
    /// Last 4 digits of card number if paid by card.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_last4: Option<String>,
    /// Reference or transaction number for card or online payment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
}

/// The `staff{}` sub-object of `CreateSaleRequest` — just the cashier's
/// display name today, kept as its own struct (rather than a bare top-level
/// field) so the request groups "who completed this sale" the same way it
/// groups "who it was sold to" (`CustomerRef`) and "how it was paid"
/// (`PaymentDetails`).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaffRef {
    /// Display name of the staff member operating the register.
    pub cashier_name: String,
}

/// Body for `POST /billing/credit-notes/{id}/void`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidCreditNoteRequest {
    /// Mandatory reason explaining why the credit note is being voided.
    pub reason: String,
}

/// Body for `POST /billing/invoices/{key}/void`. Unlike the old
/// cancel flow this collapsed from, `reason` is mandatory — a void always
/// reverses stock/payment effects that already happened, so it needs an
/// audit trail of why (see `InvoiceStatus::Voided`'s doc comment).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidInvoiceRequest {
    /// Mandatory reason explaining why the invoice is being voided.
    pub reason: String,
}
