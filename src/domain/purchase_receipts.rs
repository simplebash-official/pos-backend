// Pure business types for supplier receipts: one invoice's worth of stock received on a single
// visit or delivery. A receipt is the header; each product on it is a `Purchase` line. No I/O,
// no Mongo/Axum types beyond serde/utoipa derives.
//
// Nothing about what has been paid is stored on a receipt: the total, what has been paid and
// credited, what is still due and the status are worked out from its lines and from the
// settlements and credits that point at it every time it is read.

use crate::domain::{
    inventory::PaginationMeta,
    purchases::{Purchase, SupplierSummary},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// A receipt as returned to API clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseReceipt {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key (`prc_...`).
    pub key: String,
    /// Running number shown to people (`PUR-000001`).
    pub receipt_number: String,
    /// Foreign key referencing the supplier.
    pub supplier_key: String,
    /// The supplier's invoice or delivery-note number, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    /// Day the stock was received.
    pub date: DateTime<Utc>,
    /// Optional remarks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Enriched supplier details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplier: Option<SupplierSummary>,
    /// The products received, each with its own cost and selling price.
    pub lines: Vec<Purchase>,
    /// Sum of `quantity x unit cost` over the lines.
    pub total_cents: i64,
    /// Taken off the bill by supplier credits (short or damaged goods, price differences).
    pub credited_cents: i64,
    /// Paid against the bill, reversed payments left out.
    pub paid_cents: i64,
    /// What is still owed: the total less the credits and the payments.
    pub due_cents: i64,
    pub status: ReceiptStatus,
    /// The payments that settled this receipt, oldest first.
    pub settlements: Vec<ReceiptSettlement>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version number.
    pub version: i64,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

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

/// What kind of payment record a settlement points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaymentKind {
    /// A drawer `supplier_payment` cash movement.
    CashMovement,
    /// A payment on a ledger expense (bank, card or online).
    ExpensePayment,
}

impl PaymentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            PaymentKind::CashMovement => "cash_movement",
            PaymentKind::ExpensePayment => "expense_payment",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cash_movement" => Some(PaymentKind::CashMovement),
            "expense_payment" => Some(PaymentKind::ExpensePayment),
            _ => None,
        }
    }
}

/// One payment, or part of one, counted against a receipt.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptSettlement {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key (`rset_...`).
    pub key: String,
    pub receipt_key: String,
    pub supplier_key: String,
    pub amount_cents: i64,
    pub payment_kind: PaymentKind,
    /// Key of the cash movement or expense payment that paid.
    pub payment_key: String,
    /// True once the payment was reversed; a reversed settlement no longer counts as paid.
    pub reversed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

/// Part of a payment counted against one receipt, when paying a supplier later.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptSettlementRequest {
    pub receipt_key: String,
    /// At most what is still due on that receipt.
    pub amount_cents: i64,
}

/// How a receipt is paid at the moment it is received.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaymentSource {
    /// Cash from the register drawer.
    Drawer,
    Bank,
    Card,
    Online,
    /// The owner paid with their own money: recorded as capital put into the drawer and then
    /// paid out, so the drawer count stays right.
    OwnerPocket,
}

/// Payment made as the receipt is recorded: paid in full, or part paid when the amount is less
/// than the total. Leave the whole `payment` out for a receipt bought on credit.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptPayment {
    pub source: PaymentSource,
    /// What is paid now, at most the receipt total.
    pub amount_cents: i64,
    /// The supplier-payment category the payment is filed under.
    pub category_key: String,
    /// This register's terminal. Required for the drawer and the owner's pocket, refused for
    /// the bank, a card and online.
    #[serde(default)]
    pub terminal_id: Option<String>,
    /// The open shift the cash is booked on; omitted only between shifts.
    #[serde(default)]
    pub session_key: Option<String>,
    /// `YYYY-MM-DD` the money left the account. Required for the bank, a card and online,
    /// refused for the drawer and the owner's pocket (cash is booked when it is recorded).
    #[serde(default)]
    pub paid_on: Option<String>,
    /// A manager or owner PIN, when the payment needs one.
    #[serde(default)]
    pub manager_pin: Option<String>,
    /// Why more is paid out than the drawer holds.
    #[serde(default)]
    pub override_reason: Option<String>,
}

/// One product on a receipt being recorded.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptLineRequest {
    /// Foreign key of the product.
    pub product_key: String,
    /// Units received.
    pub quantity: i64,
    /// Cost price per unit in cents.
    pub unit_cost_cents: i64,
    /// Price each received unit sells at, in cents.
    pub selling_price_cents: i64,
    /// One serial number per unit, required (and counted against `quantity`) when the product
    /// is serialized; omitted otherwise.
    #[serde(default)]
    pub serial_numbers: Option<Vec<String>>,
}

/// Body for `POST /purchase-receipts`. Recording a receipt brings every line into stock as its
/// own batch, in one all-or-nothing save.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateReceiptRequest {
    /// Foreign key of the supplier.
    pub supplier_key: String,
    /// The supplier's invoice or delivery-note number.
    #[serde(default)]
    pub reference_no: Option<String>,
    /// Day the stock was received.
    pub date: DateTime<Utc>,
    /// Optional remarks.
    #[serde(default)]
    pub notes: Option<String>,
    /// At least one line; a product may appear on only one line.
    pub lines: Vec<ReceiptLineRequest>,
    /// What is paid as the stock is received. Absent means the whole receipt is on credit.
    #[serde(default)]
    pub payment: Option<ReceiptPayment>,
    /// Set after a `DUPLICATE_RECEIPT` warning to record the receipt anyway.
    #[serde(default)]
    pub confirm_duplicate: bool,
}

/// Query params for `GET /purchase-receipts`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ReceiptListQuery {
    /// Only this supplier's receipts.
    pub supplier_key: Option<String>,
    /// Only receipts with something still due (`true`), or only settled ones (`false`).
    pub open: Option<bool>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page limit.
    pub limit: Option<u64>,
}

/// Paginated response for `GET /purchase-receipts`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptListResponse {
    pub items: Vec<PurchaseReceipt>,
    pub pagination: PaginationMeta,
}
