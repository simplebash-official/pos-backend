use super::create_credit_note_item_request::CreateCreditNoteItemRequest;
use super::create_sale_item_request::CreateSaleItemRequest;
use super::refund_breakdown_leg::RefundBreakdownLeg;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /billing/credit-notes` — initiates a return, refund
/// (cashback), or exchange/replacement.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCreditNoteRequest {
    /// Unique business key of the invoice being returned against (e.g.
    /// inv_...). Omit (and set `noReceipt: true`) for a no-receipt return.
    #[serde(
        default,
        alias = "originalInvoiceId",
        alias = "original_invoice_id",
        alias = "invoice_id",
        alias = "invoiceId"
    )]
    pub invoice_key: Option<String>,
    /// Set when the customer has no invoice reference at all — items are
    /// valued at current selling price and this requires manager approval
    /// (`overrideReason` + an Admin caller).
    #[serde(default)]
    pub no_receipt: bool,
    /// List of items being returned.
    #[serde(alias = "items", alias = "returned_items")]
    pub returned_items: Vec<CreateCreditNoteItemRequest>,
    /// Optional replacement/exchange items the customer is taking.
    #[serde(default, alias = "exchange_items")]
    pub exchange_items: Option<Vec<CreateSaleItemRequest>>,
    /// Payment method used for cashback payout or customer extra payment
    /// ("cash", "card", "online"). Ignored when `refundBreakdown` is given.
    #[serde(
        default,
        alias = "payoutMethod",
        alias = "payout_method",
        alias = "payment_method"
    )]
    pub payment_method: Option<String>,
    /// Explicit refund allocation across payment methods — required only
    /// when the original invoice was split-paid and the default
    /// proportional split isn't what's wanted; must sum to exactly the
    /// capped cash-refund amount.
    #[serde(default)]
    pub refund_breakdown: Option<Vec<RefundBreakdownLeg>>,
    /// Reason a manager is approving this credit note outside the normal
    /// return window, or without a receipt. Required for either case;
    /// the caller must hold the Admin role.
    #[serde(default)]
    pub override_reason: Option<String>,
    /// General notes or remarks for this credit note.
    #[serde(default)]
    pub notes: Option<String>,
    /// Register session key if processed during an active cashier shift.
    #[serde(default)]
    pub session_key: Option<String>,
}
