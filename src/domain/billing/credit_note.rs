//! Credit notes (returns), their lines, conditions, reasons and requests.

use super::invoice::InvoiceItem;
use super::sale::CreateSaleItemRequest;
use crate::domain::inventory::BatchAllocation;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

fn default_return_reason() -> ReturnReason {
    ReturnReason::Other
}

/// One item in a `POST /billing/credit-notes` request.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateCreditNoteItemRequest {
    /// Product key for standard retail inventory items.
    #[serde(
        default,
        alias = "productId",
        alias = "product_id",
        alias = "product_key"
    )]
    pub product_key: Option<String>,
    /// Item name matching the invoice line if no product key (required for
    /// a no-receipt line, which has no invoice line to match against).
    pub name: Option<String>,
    /// Quantity of items being returned.
    pub quantity: i64,
    /// Reason for return.
    #[serde(default = "default_return_reason")]
    pub reason: ReturnReason,
    /// The item's physical/sellable condition.
    #[serde(alias = "itemCondition")]
    pub condition: ItemCondition,
    /// What happens to the item — required iff `condition == "damaged"`.
    pub disposition: Option<ItemDisposition>,
    /// The specific serialized unit being returned, if applicable.
    pub serial_number: Option<String>,
    /// Optional notes or reason description.
    pub notes: Option<String>,
    /// Unit price in cents for custom/ad-hoc lines.
    #[serde(alias = "unit_price_cents")]
    pub unit_price_cents: Option<i64>,
    /// For a no-receipt product line: the price the item was sold at, picked
    /// from the product's prices. The return is valued at it and restocked
    /// into the newest batch sold at it.
    pub selling_price_cents: Option<i64>,
    /// Source ticket key if returning a repair/print line.
    #[serde(alias = "source_ticket_key")]
    pub source_ticket_key: Option<String>,
}

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
    #[serde(alias = "exchange_items")]
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
    pub refund_breakdown: Option<Vec<RefundBreakdownLeg>>,
    /// Reason a manager is approving this credit note outside the normal
    /// return window, or without a receipt. Required for either case;
    /// the caller must hold the Admin role.
    pub override_reason: Option<String>,
    /// General notes or remarks for this credit note.
    pub notes: Option<String>,
    /// Register session key if processed during an active cashier shift.
    pub session_key: Option<String>,
    /// The warranty claim this refund settles (`wcl_…`). It must have been decided as a
    /// refund on the same invoice, and a serialized item must be among what comes back. A
    /// manager already approved it, so the return window does not apply.
    pub warranty_claim_key: Option<String>,
}

/// A return/refund/exchange transaction record.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreditNote {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Unique business key of the credit note (e.g. cn_...).
    pub key: String,
    /// Human-friendly sequential credit note number (e.g. CN-000001).
    pub credit_note_number: String,
    /// Unique business key of the associated invoice, if any (absent for a
    /// no-receipt credit note).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_key: Option<String>,
    /// Human-friendly invoice number, if `invoiceKey` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_number: Option<String>,
    /// True if this credit note has no linked invoice.
    pub no_receipt: bool,
    /// Key of the customer linked to the original invoice, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Customer's name at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name_snapshot: Option<String>,
    /// Staff ID of cashier who processed the credit note.
    pub cashier_id: String,
    /// Cashier's display name at the time of the credit note.
    pub cashier_name_snapshot: String,
    /// List of returned items.
    pub returned_items: Vec<CreditNoteItem>,
    /// List of replacement or exchange items provided, if any.
    pub exchange_items: Vec<InvoiceItem>,
    /// Set whenever `exchangeItems` is non-empty — lets reporting
    /// distinguish a genuine return (money leaving, nothing replacing it)
    /// from an exchange (product swapped, net-neutral or near-neutral).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exchange_reference: Option<String>,
    /// Total monetary value of returned items in cents.
    pub return_subtotal_cents: i64,
    /// Total monetary value of replacement/exchange items in cents.
    pub exchange_subtotal_cents: i64,
    /// Net refund amount in cents (positive = cashback to customer,
    /// negative = customer pays extra) before the partial-payment cap.
    pub net_refund_cents: i64,
    /// The portion of `netRefundCents` actually paid out (or collected, if
    /// negative) in cash/card/online — capped at what was actually paid on
    /// the invoice so far. Equal to `netRefundCents` whenever no cap
    /// applies (no invoice, or the invoice was fully paid).
    pub refund_cash_cents: i64,
    /// The remainder of a positive `netRefundCents` that couldn't be paid
    /// out in cash because the invoice hadn't been paid that much yet —
    /// reduces what the invoice still owes instead of being handed over as
    /// cash the shop never received.
    pub balance_reduction_cents: i64,
    /// How `refundCashCents` was allocated across payment methods.
    pub refund_breakdown: Vec<RefundBreakdownLeg>,
    /// Keys of payment record(s) created for the refund or extra payment, if any.
    pub refund_payment_keys: Vec<String>,
    /// This credit note's resolution state.
    pub status: CreditNoteStatus,
    /// True when this credit note was approved past the normal return
    /// window, or as a no-receipt return.
    pub is_manager_override: bool,
    /// User ID of the Admin who approved the override, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_approved_by: Option<String>,
    /// The reason given for the override, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_reason: Option<String>,
    /// General notes or remarks for this credit note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Register session key this credit note belongs to, if processed during an open session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    /// Time when this credit note was voided, if voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_at: Option<DateTime<Utc>>,
    /// ID of the user who voided this credit note.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_by: Option<String>,
    /// Mandatory reason this credit note was voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_reason: Option<String>,
    /// Timestamp when the credit note was recorded.
    pub created_at: DateTime<Utc>,
    /// Timestamp when the credit note was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version number.
    pub version: i64,
}

/// A line item in a credit note representing a returned product or service.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreditNoteItem {
    /// Foreign key referencing the catalog product, if this is a retail item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    /// Display name or description of the returned item.
    pub name: String,
    /// Stock Keeping Unit (SKU) barcode identifier, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    /// Number of units returned.
    pub quantity: i64,
    /// Price per unit in cents refunded.
    pub unit_price_cents: i64,
    /// Total price for this line item in cents refunded.
    pub total_cents: i64,
    /// Reason why the item was returned.
    pub reason: ReturnReason,
    /// The item's physical/sellable condition.
    pub condition: ItemCondition,
    /// What happens to the item — required iff `condition == Damaged`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disposition: Option<ItemDisposition>,
    /// The specific serialized unit returned, if the original line was for
    /// a serialized product.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial_number: Option<String>,
    /// Whether the returned serial was still within its warranty period at
    /// the time of return. Response-only, computed from the matched
    /// `ProductSerial`; absent for non-serialized items.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub within_warranty: Option<bool>,
    /// Optional notes or defect details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Source category: `"retail"`, `"repair"`, `"print"`, or `"charge"` (a one-time charge with no
    /// stock, ticket or cost on record).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    /// Key of linked repair or print job ticket, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_key: Option<String>,
    /// Formatted ticket number of linked repair or print job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_number: Option<String>,
    /// The stock batches a product line's units belong back in: the sold
    /// line's own batches (newest first) for a receipt return, the newest
    /// batch at the picked price for a no-receipt return. Units are only put
    /// back for a restockable condition; voiding the credit note takes them
    /// out of these same batches again.
    pub batch_allocations: Vec<BatchAllocation>,
}

/// Query parameters for listing and filtering credit note records.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct CreditNoteListQuery {
    /// Search term matching credit note number, invoice number, or customer name.
    pub search: Option<String>,
    /// Filter by original invoice key.
    pub invoice_key: Option<String>,
    /// Filter by customer key.
    pub customer_key: Option<String>,
    /// Scopes to credit notes created today when set to `"today"`.
    pub date_preset: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Page item limit.
    pub limit: Option<u64>,
}

/// Paginated response payload containing list of credit notes.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreditNoteListResponse {
    /// List of credit notes for the requested page.
    pub credit_notes: Vec<CreditNote>,
    /// Total count of matching credit notes.
    pub total: u64,
    /// Current page number.
    pub page: u64,
    /// Maximum items per page.
    pub limit: u64,
    /// Total number of pages available.
    pub total_pages: u64,
}

/// A credit note's overall resolution state. `AwaitingResolution` is set
/// only when at least one line is `ItemCondition::PendingInspection` — this
/// codebase has no edit-in-place endpoint for any billing document, so
/// there is deliberately no way to move a credit note out of
/// `AwaitingResolution` other than `Voided` in this pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreditNoteStatus {
    Resolved,
    AwaitingResolution,
    Voided,
}

/// The physical/sellable condition of a returned item, independent of why
/// it was returned. Drives which `StockMovementType` (if any)
/// `service::credit_notes::create_credit_note` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ItemCondition {
    /// Unopened/undamaged — goes straight back into sellable stock.
    Resalable,
    /// Broken, faulty, or otherwise unsellable as new — requires an
    /// `ItemDisposition`.
    Damaged,
    /// Works, but opened/used — optionally restocked at a discount.
    OpenBoxDiscount,
    /// Returned but not yet checked — no stock movement happens until this
    /// is resolved to one of the other conditions (out of scope for this
    /// pass, see `CreditNoteStatus::AwaitingResolution`).
    PendingInspection,
}

/// What happens to a `Damaged` item. Required whenever `condition ==
/// Damaged`, forbidden otherwise — validated in
/// `service::credit_notes::create_credit_note`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ItemDisposition {
    /// Logged for a supplier RMA/replacement claim — no stock quantity change.
    ReturnToSupplier,
    /// Discarded as inventory loss/shrinkage — no stock quantity change.
    WriteOffScrap,
    /// Held for repair; out of scope for this pass what happens once
    /// repaired (no stock movement is written).
    RepairPending,
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

/// Body for `POST /billing/credit-notes/{id}/void`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidCreditNoteRequest {
    /// Mandatory reason explaining why the credit note is being voided.
    pub reason: String,
}
