// Pure business types for the billing feature — no I/O, no Mongo/Axum types
// beyond serde/utoipa derives. Mongo document shapes live in
// `modules::billing::model` and convert into these before a handler wraps
// them in `core::response::ApiResponse<T>`.
//
// Invoices and payments are append-only (see D3 in the migration plan): no
// general edit endpoint exists. `Invoice`/`PaymentRecord` are returned from
// create/list/get/cancel/record-payment only.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// One line of a sale, embedded in both the complete-sale request and the
/// persisted/returned `Invoice`. `source_ticket_key` (the repair/print-job's
/// Mongo-independent `key`, e.g. `rep_...`) is what `complete_sale` uses to
/// mark a ticket delivered — `source_ticket_number` (`REP-000001`) is
/// display-only, shown on receipts/invoices and in the cart.
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
    /// `"paid"` | `"pending"` | `"cancelled"`.
    pub status: String,
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
    /// Time when the invoice was cancelled, if cancelled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<DateTime<Utc>>,
    /// ID of the user who cancelled the invoice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled_by: Option<String>,
    /// Reason provided for invoice cancellation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_reason: Option<String>,
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
/// credit repayment).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRecord {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key of the payment (e.g. pay_...).
    pub key: String,
    /// Key of the invoice this payment was credited towards.
    pub invoice_key: String,
    /// Payment amount in cents.
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

/// Body for `POST /billing/invoices/{key}/cancel` — see D8 in the migration
/// plan for the basic version's guard (blocked once any payment beyond the
/// original sale-time one has been recorded).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CancelInvoiceRequest {
    /// Reason explaining why the invoice is being cancelled.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Query parameters for listing and filtering invoices.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct InvoiceListQuery {
    /// Search term matching invoice number or customer name/phone.
    pub search: Option<String>,
    /// Filter by status ("paid", "pending", "cancelled").
    pub status: Option<String>,
    /// Filter by customer key.
    pub customer_key: Option<String>,
    /// `"paid"` (status == "paid" AND NOT credit) or `"credit"` (is_credit OR
    /// status == "pending") — the same compound rule `BillingStats.
    /// outstandingCreditCents` uses, exposed here so the Sales & Invoices
    /// History screen's Paid/Credit toggle can filter server-side.
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
    /// on credit (`isCredit == true`) or still `status == "pending"`.
    pub outstanding_credit_cents: i64,
    /// `todaySalesCents / todayInvoiceCount`, rounded; 0 when there were no
    /// invoices today.
    pub avg_basket_cents: i64,
}
