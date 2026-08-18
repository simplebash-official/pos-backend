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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    pub unit_price_cents: i64,
    pub quantity: i64,
    pub discount_cents: i64,
    pub total_cents: i64,
    /// `"retail"` | `"repair"` | `"print"`.
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ticket_number: Option<String>,
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
    #[serde(default)]
    pub product_key: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub unit_price_cents: Option<i64>,
    pub quantity: i64,
    pub discount_cents: i64,
    /// `"retail"` | `"repair"` | `"print"`.
    pub source_type: String,
    #[serde(default)]
    pub source_ticket_key: Option<String>,
    #[serde(default)]
    pub assigned_employee_name: Option<String>,
}

/// One leg of a split payment. `method` is `"cash"` | `"card"` | `"online"`
/// (never `"split"` — that's the invoice-level `payment_method` value that
/// means "see `split_payments`").
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SplitPayment {
    pub method: String,
    pub amount_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_last4: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
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
    pub id: String,
    pub key: String,
    pub invoice_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_name_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_phone_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customer_address_snapshot: Option<String>,
    pub cashier_id: String,
    pub cashier_name_snapshot: String,
    pub items: Vec<InvoiceItem>,
    pub subtotal_cents: i64,
    pub discount_cents: i64,
    pub tax_cents: i64,
    pub total_cents: i64,
    /// `"cash"` | `"card"` | `"online"` | `"split"` | `"credit"`.
    pub payment_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_payments: Option<Vec<SplitPayment>>,
    pub is_credit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount_received_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_due_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_last4: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub online_note: Option<String>,
    /// `"paid"` | `"pending"` | `"cancelled"`.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub shop_profile_snapshot: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_terms_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_selection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancelled_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default = "default_version")]
    pub version: i64,
}

fn default_version() -> i64 {
    1
}

/// Body for `POST /billing/sales`. `cashierId` is deliberately absent —
/// the completing cashier is always `CurrentUser::user_id` from the bearer
/// token, never client-supplied. `customerName`/`Phone`/`Address` are only
/// used as the walk-in snapshot when `customerKey` is absent; when a real
/// customer is linked, `service::sale::complete_sale` snapshots from the
/// resolved `Customer` record instead (server-authoritative, not
/// client-supplied) — see the migration plan's D2/D4 notes.
///
/// `subtotalCents`/`totalCents`/`changeDueCents` are never client-supplied
/// either — all three are arithmetic derived from data the server already
/// has once items are resolved: `subtotalCents` is the sum of the resolved
/// items' `totalCents`, `totalCents` is `subtotalCents - discountCents +
/// taxCents`, and `changeDueCents` (when `amountReceivedCents` is given) is
/// `amountReceivedCents - totalCents`. `discountCents`/`taxCents` stay
/// client-supplied — an invoice-level discount/tax isn't derivable from
/// anything already stored server-side.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSaleRequest {
    #[serde(default)]
    pub customer_key: Option<String>,
    #[serde(default)]
    pub customer_name: Option<String>,
    #[serde(default)]
    pub customer_phone: Option<String>,
    #[serde(default)]
    pub customer_address: Option<String>,
    pub cashier_name: String,
    pub items: Vec<CreateSaleItemRequest>,
    pub discount_cents: i64,
    pub tax_cents: i64,
    pub payment_method: String,
    #[serde(default)]
    pub split_payments: Option<Vec<SplitPayment>>,
    pub is_credit: bool,
    #[serde(default)]
    pub amount_received_cents: Option<i64>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub card_last4: Option<String>,
    #[serde(default)]
    pub card_ref: Option<String>,
    #[serde(default)]
    pub online_ref: Option<String>,
    #[serde(default)]
    pub online_note: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    pub shop_profile_snapshot: serde_json::Value,
    #[serde(default)]
    pub warranty_terms_snapshot: Option<String>,
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
    pub invoice: Invoice,
    pub payments: Vec<PaymentRecord>,
    pub warnings: Vec<String>,
}

/// A payment applied against an invoice — either recorded at sale-completion
/// time or later via `POST /billing/invoices/{key}/payments` (partial
/// credit repayment).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRecord {
    pub id: String,
    pub key: String,
    pub invoice_key: String,
    pub amount_cents: i64,
    pub payment_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub recorded_by_user_id: String,
    pub recorded_by_name_snapshot: String,
    pub recorded_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default = "default_version")]
    pub version: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordPaymentRequest {
    pub amount_cents: i64,
    pub payment_method: String,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PaymentListResponse {
    pub payments: Vec<PaymentRecord>,
}

/// Body for `POST /billing/invoices/{key}/cancel` — see D8 in the migration
/// plan for the basic version's guard (blocked once any payment beyond the
/// original sale-time one has been recorded).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CancelInvoiceRequest {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct InvoiceListQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub customer_key: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceListResponse {
    pub invoices: Vec<Invoice>,
    pub total: u64,
    pub page: u64,
    pub limit: u64,
    pub total_pages: u64,
}
