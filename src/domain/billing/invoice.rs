//! Invoices, their lines and status, list queries, voiding, and the shared customer and staff references.

use super::payment::SplitPayment;
use crate::domain::inventory::BatchAllocation;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Response for `GET /billing/invoices/stats` — the 4 KPI cards on the
/// frontend's Sales & Invoices History screen, computed server-side so
/// online and offline clients always see the same numbers.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BillingStats {
    /// Sum of `totalCents` across invoices created today (UTC day boundary).
    pub today_sales_cents: i64,
    /// Count of invoices created today.
    pub today_invoice_count: u64,
    /// Sum of `totalCents` across ALL invoices (any date) that are either
    /// on credit (`isCredit == true`) or still `status` in (pending,
    /// partially_paid).
    pub outstanding_credit_cents: i64,
    /// `todaySalesCents / todayInvoiceCount`, rounded; 0 when there were no
    /// invoices today.
    pub avg_basket_cents: i64,
}

/// Whether an invoice should be flagged overdue: still awaiting (full)
/// payment and its `due_date` has passed. Computed fresh on every read
/// (`InvoiceDocument::into_invoice`) rather than stored, since it's a pure
/// function of two fields that are already persisted.
#[must_use]
pub fn compute_is_overdue(status: InvoiceStatus, due_date: Option<&str>) -> bool {
    if !matches!(
        status,
        InvoiceStatus::Pending | InvoiceStatus::PartiallyPaid
    ) {
        return false;
    }
    let Some(due_date) = due_date else {
        return false;
    };
    match NaiveDate::parse_from_str(due_date, "%Y-%m-%d") {
        Ok(due) => due < Utc::now().date_naive(),
        Err(_) => false,
    }
}

/// The `customer{}` sub-object of `CreateSaleRequest`. `customerName`/
/// `Phone`/`Address` are only used as the walk-in snapshot when
/// `customerKey` is absent; when a real customer is linked,
/// `service::sale::complete_sale` snapshots from the resolved `Customer`
/// record instead (server-authoritative, not client-supplied).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerRef {
    /// Key of an existing registered customer.
    pub customer_key: Option<String>,
    /// Walk-in customer name if not registered.
    pub customer_name: Option<String>,
    /// Walk-in customer phone number if not registered.
    pub customer_phone: Option<String>,
    /// Walk-in customer address if not registered.
    pub customer_address: Option<String>,
}

/// A completed or pending sale as returned to API clients. `id` is the
/// Mongo `ObjectId` hex string; `key` (`inv_...`) is the prefixed id other
/// modules/the frontend reference this invoice by. `shop_profile_snapshot`
/// is the frontend's full `ShopProfile` object, embedded verbatim at
/// creation time (D2) — reprinting this invoice's documents always uses
/// these frozen shop details, never "whatever the shop profile says today".
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Invoice {
    /// MongoDB internal hex ID.
    pub id: String,
    /// Unique business key of the invoice (e.g. inv_...).
    pub key: String,
    /// Human-friendly sequential invoice number (e.g. INV-000001).
    pub invoice_number: String,
    /// Key of the customer if linked to a registered account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    /// Customer's name at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name_snapshot: Option<String>,
    /// Customer's phone at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone_snapshot: Option<String>,
    /// Customer's address at the time of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_address_snapshot: Option<String>,
    /// Staff ID of cashier who processed the invoice.
    pub cashier_id: String,
    /// Cashier's display name at the time of sale.
    pub cashier_name_snapshot: String,
    /// Key of the active register session when invoice was issued.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_key: Option<String>,
    /// Purchased line items and services.
    pub items: Vec<InvoiceItem>,
    /// Subtotal price before discounts in cents.
    pub subtotal_cents: i64,
    /// `"percentage"` | `"fixed"` — the discount type this invoice's
    /// `discountCents` was computed from (`"fixed"`/`0.0` when no discount
    /// was applied). Kept alongside the computed cents so a receipt can
    /// show "5% off" rather than just the resulting amount.
    pub discount_type: String,
    /// Numeric rate or flat value of discount applied.
    pub discount_value: f64,
    /// Discount amount deducted in cents.
    pub discount_cents: i64,
    /// Final total amount payable in cents.
    pub total_cents: i64,
    /// `"cash"` | `"card"` | `"online"` | `"split"` | `"credit"`.
    pub payment_method: String,
    /// Breakdown list of payments if split method used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_payments: Option<Vec<SplitPayment>>,
    /// True if sale was performed on credit/tab.
    pub is_credit: bool,
    /// Total amount received from customer in cash in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_received_cents: Option<i64>,
    /// Change given back to customer in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_due_cents: Option<i64>,
    /// Due date for payment if invoice is on credit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    /// Last 4 digits of card used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_last4: Option<String>,
    /// Card transaction reference code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_ref: Option<String>,
    /// Online payment transaction reference code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_ref: Option<String>,
    /// Online payment note or bank reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_note: Option<String>,
    /// The invoice's current lifecycle state.
    pub status: InvoiceStatus,
    /// Whether `status`'s due date has passed while still `Pending`/
    /// `PartiallyPaid` — computed at read time, never stored.
    pub is_overdue: bool,
    /// Customer-facing or internal invoice notes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Snapshot of shop branding and contact details when invoice was issued.
    pub shop_profile_snapshot: serde_json::Value,
    /// Warranty policy statement printed on receipt/invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_terms_snapshot: Option<String>,
    /// Receipt layout or document template format selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_selection: Option<String>,
    /// Time when the invoice was voided, if voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_at: Option<DateTime<Utc>>,
    /// ID of the user who voided the invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_by: Option<String>,
    /// Mandatory reason the invoice was voided.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voided_reason: Option<String>,
    /// Time when the invoice was closed, if closed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,
    /// ID of the user who closed the invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed_by: Option<String>,
    /// Total amount in cents refunded against this invoice from credit notes.
    pub refunded_cents: i64,
    /// Count of credit notes (any status) recorded against this invoice —
    /// display-only; `Closed`'s "no open credit note" guard re-queries the
    /// `credit_notes` collection rather than trusting this count alone.
    pub credit_note_count: i64,
    /// Timestamp when invoice was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when invoice was last updated.
    pub updated_at: DateTime<Utc>,
    /// Device identifier that created or last updated this invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
    /// Optimistic locking version number.
    pub version: i64,
}

/// One line of a sale, embedded in both the complete-sale request and the
/// persisted/returned `Invoice`. `source_ticket_key` (the repair/print-job's
/// Mongo-independent `key`, e.g. `rep_...`) is what `complete_sale` uses to
/// mark a ticket delivered — `source_ticket_number` (`REP-000001`) is
/// display-only, shown on receipts/invoices and in the cart.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceItem {
    /// Foreign key referencing the catalog product, if this is a retail item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    /// Display name or description of the sold item or service.
    pub name: String,
    /// Stock Keeping Unit (SKU) barcode identifier, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    /// Price per unit in cents.
    pub unit_price_cents: i64,
    /// Number of units purchased.
    pub quantity: i64,
    /// Discount amount given on this specific item in cents.
    pub discount_cents: i64,
    /// Total price for this line item in cents after discount.
    pub total_cents: i64,
    /// Snapshot of the unit cost at sale time, in cents. For a retail line
    /// this is the weighted cost of the stock batches the units came from
    /// (see `batch_allocations`);
    /// for a repair/print line it is the linked ticket's `material_cost_cents`
    /// divided across the line quantity. `None` on an ad-hoc line with no
    /// product/ticket to resolve against, and on every invoice created before
    /// this field existed — treat a missing value as "cost unknown", never as
    /// zero, in any margin calculation that must not silently understate cost.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_cost_cents: Option<i64>,
    /// Source category: `"retail"`, `"repair"`, or `"print"`.
    pub source_type: String,
    /// Key of linked repair or print job ticket, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_key: Option<String>,
    /// Formatted ticket number of linked repair or print job (e.g. REP-000001).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_number: Option<String>,
    /// Display name of technician or operator who handled this ticket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_employee_name: Option<String>,
    /// Quantity of items already returned from this line item.
    pub returned_quantity: i64,
    /// The specific serialized units sold on this line, if the product is
    /// serialized (see `Product.is_serialized`) — one entry per unit,
    /// `quantity` long. Empty for a non-serialized retail line and for
    /// repair/print lines.
    pub serial_numbers: Vec<String>,
    /// Which stock batches a retail product line's units came out of (one
    /// line at one price can span several batches, oldest first). A void or
    /// a return puts units back into exactly these batches. Empty for
    /// repair/print and ad-hoc lines, which move no stock.
    pub batch_allocations: Vec<BatchAllocation>,
}

/// Query parameters for listing and filtering invoices.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct InvoiceListQuery {
    /// Search term matching invoice number or customer name/phone.
    pub search: Option<String>,
    /// Filter by status ("pending", "partially_paid", "paid", "voided",
    /// "closed"), or the synthetic value "overdue" (translated server-side
    /// to `status in (pending, partially_paid) AND due_date < today`).
    pub status: Option<String>,
    /// Filter by customer key.
    pub customer_key: Option<String>,
    /// `"paid"` (status == "paid" AND NOT credit) or `"credit"` (is_credit
    /// OR status in (pending, partially_paid)) — the same compound rule
    /// `BillingStats.outstandingCreditCents` uses, exposed here so the
    /// Sales & Invoices History screen's Paid/Credit toggle can filter
    /// server-side.
    pub payment_status: Option<String>,
    /// Exact match on payment method ("cash", "card", "online", "split").
    pub payment_method: Option<String>,
    /// Only `"today"` is meaningful — scopes to `created_at` within
    /// `shop_today_range()`. Any other value (or absence) means all time.
    pub date_preset: Option<String>,
    /// Filter by register shift session key.
    pub session_key: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Page item limit.
    pub limit: Option<u64>,
}

/// Paginated response payload containing invoice list.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceListResponse {
    /// List of invoices for the requested page.
    pub invoices: Vec<Invoice>,
    /// Total count of matching invoices.
    pub total: u64,
    /// Current page number.
    pub page: u64,
    /// Maximum items per page.
    pub limit: u64,
    /// Total number of pages available.
    pub total_pages: u64,
}

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
    /// The name this value has on the wire and in storage.
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

/// `discountType` must be `"percentage"` (`discountValue` is a 0–100
/// percent, e.g. `7.5`) or `"fixed"` (`discountValue` is a plain cents
/// amount) — `service::sale::complete_sale` computes the actual
/// `discountCents` from this against the resolved subtotal, and persists
/// both this type/value (source of truth) and the resulting cents on the
/// `Invoice`. Omitting `pricingAdjustments` entirely means no discount.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PricingAdjustments {
    /// Discount method ("percentage" or "fixed").
    pub discount_type: String,
    /// Discount numeric value (e.g. 10.0 for 10% or 500 for $5.00 fixed).
    pub discount_value: f64,
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
