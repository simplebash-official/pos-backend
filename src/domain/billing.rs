// Pure business types for the billing feature — no I/O, no Mongo/Axum types
// beyond serde/utoipa derives. Mongo document shapes live in
// `modules::billing::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.
//
// Invoices and payments are append-only (see D3 in the migration plan): no
// general edit endpoint exists. `Invoice`/`PaymentRecord` are returned from
// create/list/get/void/close/record-payment only. Credit notes follow the
// same append-only posture: a credit note is created once as a single
// atomic write (never edited), with `void_credit_note` as the only
// after-the-fact reversal.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

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
    /// this is the product's `cost_price_cents` when the sale was completed;
    /// for a repair/print line it is the linked ticket's `material_cost_cents`
    /// divided across the line quantity. `None` on an ad-hoc line with no
    /// product/ticket to resolve against, and on every invoice created before
    /// this field existed — treat a missing value as "cost unknown", never as
    /// zero, in any margin calculation that must not silently understate cost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default)]
    pub returned_quantity: i64,
    /// The specific serialized units sold on this line, if the product is
    /// serialized (see `Product.is_serialized`) — one entry per unit,
    /// `quantity` long. Empty for a non-serialized retail line and for
    /// repair/print lines.
    #[serde(default)]
    pub serial_numbers: Vec<String>,
}

/// One line of a `POST /billing/sales` request. Deliberately leaner than
/// `InvoiceItem`: `name`/`unitPriceCents` (and, for repair/print lines,
/// `assignedEmployeeName`) are only accepted here as a fallback for an
/// ad-hoc line with no key to resolve against — `sku`/`totalCents`/
/// `sourceTicketNumber` are never client-supplied at all, since they're
/// always derivable once the key resolves. `service::sale::complete_sale`
/// resolves each item from `productKey`/`sourceTicketKey` before any write;
/// an unresolvable key fails the whole request, the same way a bad
/// `customerKey` does.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSaleItemRequest {
    /// Product key for standard retail inventory items.
    #[serde(default)]
    pub product_key: Option<String>,
    /// Custom item name for ad-hoc/custom retail items.
    #[serde(default)]
    pub name: Option<String>,
    /// Unit price in cents for ad-hoc/custom items.
    #[serde(default)]
    pub unit_price_cents: Option<i64>,
    /// Quantity of items being purchased.
    pub quantity: i64,
    /// Discount allocated directly to this line item in cents.
    pub discount_cents: i64,
    /// Line type: `"retail"`, `"repair"`, or `"print"`.
    pub source_type: String,
    /// Ticket key if checking out a completed repair or print job.
    #[serde(default)]
    pub source_ticket_key: Option<String>,
    /// Employee assigned to the job if applicable.
    #[serde(default)]
    pub assigned_employee_name: Option<String>,
    /// The specific serialized units being sold, required (exact-count
    /// validated against `quantity`) when the resolved product is
    /// serialized; omitted for non-serialized retail lines and for
    /// repair/print lines.
    #[serde(default)]
    pub serial_numbers: Option<Vec<String>>,
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

/// The `customer{}` sub-object of `CreateSaleRequest`. `customerName`/
/// `Phone`/`Address` are only used as the walk-in snapshot when
/// `customerKey` is absent; when a real customer is linked,
/// `service::sale::complete_sale` snapshots from the resolved `Customer`
/// record instead (server-authoritative, not client-supplied).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomerRef {
    /// Key of an existing registered customer.
    #[serde(default)]
    pub customer_key: Option<String>,
    /// Walk-in customer name if not registered.
    #[serde(default)]
    pub customer_name: Option<String>,
    /// Walk-in customer phone number if not registered.
    #[serde(default)]
    pub customer_phone: Option<String>,
    /// Walk-in customer address if not registered.
    #[serde(default)]
    pub customer_address: Option<String>,
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

/// The `payment{}` sub-object of `CreateSaleRequest` — every field that
/// describes how the sale was (or will be) paid for, grouped together
/// rather than scattered across the request's top level.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentDetails {
    /// Selected payment method: `"cash"`, `"card"`, `"online"`, `"split"`, or `"credit"`.
    pub payment_method: String,
    /// Indicates if this transaction is recorded as unpaid credit.
    pub is_credit: bool,
    /// Total amount of money handed in by the customer in cents.
    #[serde(default)]
    pub amount_received_cents: Option<i64>,
    /// List of payment breakdown portions if payment method is "split".
    #[serde(default)]
    pub split_payments: Option<Vec<SplitPayment>>,
    /// Last 4 digits of payment card if paid by card.
    #[serde(default)]
    pub card_last4: Option<String>,
    /// Authorization code or reference number from card terminal.
    #[serde(default)]
    pub card_ref: Option<String>,
    /// Transaction ID or reference number for online payment.
    #[serde(default)]
    pub online_ref: Option<String>,
    /// Optional note or description for online transfer.
    #[serde(default)]
    pub online_note: Option<String>,
    /// Due date for credit invoice repayment (YYYY-MM-DD).
    #[serde(default)]
    pub due_date: Option<String>,
    /// How an up-front partial payment on a credit sale was taken:
    /// `"cash"` or `"card"`. Only meaningful when `is_credit` is true and
    /// `amount_received_cents` is a non-zero amount below the total — it
    /// selects the method recorded on the deposit's payment row. The
    /// invoice's own `payment_method` still stays `"credit"`.
    #[serde(default)]
    pub deposit_method: Option<String>,
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

/// Whether an invoice should be flagged overdue: still awaiting (full)
/// payment and its `due_date` has passed. Computed fresh on every read
/// (`InvoiceDocument::into_invoice`) rather than stored, since it's a pure
/// function of two fields that are already persisted.
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
    #[serde(default)]
    pub refunded_cents: i64,
    /// Count of credit notes (any status) recorded against this invoice —
    /// display-only; `Closed`'s "no open credit note" guard re-queries the
    /// `credit_notes` collection rather than trusting this count alone.
    #[serde(default)]
    pub credit_note_count: i64,
    /// Timestamp when invoice was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when invoice was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version number.
    #[serde(default = "default_version")]
    pub version: i64,
}

fn default_version() -> i64 {
    1
}

/// Body for `POST /billing/sales`, grouped into sub-objects by concern
/// rather than one flat field list: `staff` (who completed it), `customer`
/// (who it was sold to, optional — a walk-in with no linked account omits
/// it entirely), `items`, `pricingAdjustments` (the invoice-level
/// discount, optional — omit for no discount), `payment` (how it was/will
/// be paid), and `shopProfileSnapshot` (frozen shop details for reprints,
/// left top-level since it isn't really "about" any one sub-concern).
/// `notes`/`warrantyTermsSnapshot`/`documentSelection` stay flat top-level
/// fields too — each is a single independent optional value with no
/// natural group to join.
///
/// `cashierId` is deliberately absent from `staff` — the completing cashier
/// is always `CurrentUser::user_id` from the bearer token, never
/// client-supplied.
///
/// `subtotalCents`/`totalCents`/`changeDueCents` are never client-supplied
/// — all three are arithmetic derived from data the server already has
/// once items are resolved: `subtotalCents` is the sum of the resolved
/// items' `totalCents`, `discountCents` is computed from
/// `pricingAdjustments` against that subtotal, `totalCents` is
/// `subtotalCents - discountCents` (there is no tax concept in this
/// system), and `changeDueCents` (when `payment.amountReceivedCents` is
/// given) is `amountReceivedCents - totalCents`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSaleRequest {
    /// Cashier staff details.
    pub staff: StaffRef,
    /// Customer information or reference key.
    #[serde(default)]
    pub customer: Option<CustomerRef>,
    /// List of sale line items.
    pub items: Vec<CreateSaleItemRequest>,
    /// Optional sale-wide discount setting.
    #[serde(default)]
    pub pricing_adjustments: Option<PricingAdjustments>,
    /// Payment method and amounts.
    pub payment: PaymentDetails,
    /// Optional note printed on invoice.
    #[serde(default)]
    pub notes: Option<String>,
    /// Snapshot of current shop settings for receipt formatting.
    pub shop_profile_snapshot: serde_json::Value,
    /// Warranty conditions to snapshot onto the invoice.
    #[serde(default)]
    pub warranty_terms_snapshot: Option<String>,
    /// Document template style preference.
    #[serde(default)]
    pub document_selection: Option<String>,
}

/// Response for `POST /billing/sales`. `warnings` surfaces any sub-step
/// that failed after the invoice/payment were already durably recorded
/// (stock decrement, ticket status update, customer balance update) — see
/// D4 in the migration plan for why these never fail the whole request.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompleteSaleResponse {
    /// Created invoice record.
    pub invoice: Invoice,
    /// Recorded payment transactions for this sale.
    pub payments: Vec<PaymentRecord>,
    /// Non-fatal warnings encountered during post-sale processing.
    pub warnings: Vec<String>,
}

/// A payment applied against an invoice — either recorded at sale-completion
/// time or later via `POST /billing/invoices/{key}/payments` (partial
/// credit repayment), or as a credit-note refund/extra-payment leg.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRecord {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key of the payment (e.g. pay_...).
    pub key: String,
    /// Key of the invoice this payment was credited towards.
    pub invoice_key: String,
    /// Payment amount in cents. Negative for a credit-note cash refund.
    pub amount_cents: i64,
    /// Payment method used ("cash", "card", "online").
    pub payment_method: String,
    /// Optional notes or remarks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// User ID of employee who accepted the payment.
    pub recorded_by_user_id: String,
    /// Display name snapshot of employee who accepted payment.
    pub recorded_by_name_snapshot: String,
    /// Timestamp when payment occurred.
    pub recorded_at: DateTime<Utc>,
    /// Timestamp when payment record was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when payment record was last modified.
    pub updated_at: DateTime<Utc>,
    /// Version number for concurrency control.
    #[serde(default = "default_version")]
    pub version: i64,
}

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
}

/// Response payload containing all payments recorded against an invoice.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentListResponse {
    /// List of payments recorded against the invoice.
    pub payments: Vec<PaymentRecord>,
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
    /// `today_utc_range()`. Any other value (or absence) means all time.
    pub date_preset: Option<String>,
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

fn default_return_reason() -> ReturnReason {
    ReturnReason::Other
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
    /// Source category: `"retail"`, `"repair"`, or `"print"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    /// Key of linked repair or print job ticket, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_key: Option<String>,
    /// Formatted ticket number of linked repair or print job.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_number: Option<String>,
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
    #[serde(default)]
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
    #[serde(default)]
    pub disposition: Option<ItemDisposition>,
    /// The specific serialized unit being returned, if applicable.
    #[serde(default)]
    pub serial_number: Option<String>,
    /// Optional notes or reason description.
    #[serde(default)]
    pub notes: Option<String>,
    /// Unit price in cents for custom/ad-hoc lines.
    #[serde(default, alias = "unit_price_cents")]
    pub unit_price_cents: Option<i64>,
    /// Source ticket key if returning a repair/print line.
    #[serde(default, alias = "source_ticket_key")]
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refund_breakdown: Vec<RefundBreakdownLeg>,
    /// Keys of payment record(s) created for the refund or extra payment, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
    #[serde(default = "default_version")]
    pub version: i64,
}

/// Body for `POST /billing/credit-notes/{id}/void`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VoidCreditNoteRequest {
    /// Mandatory reason explaining why the credit note is being voided.
    pub reason: String,
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
