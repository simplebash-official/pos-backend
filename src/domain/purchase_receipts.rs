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
    /// What the lines cost, before any extra charge: the sum of the lines' totals.
    pub goods_cents: i64,
    /// Extra charges the supplier put on the bill, discounts taken off (signed). Adds to what is owed.
    pub extra_charges_cents: i64,
    /// Extra charges paid to someone other than the supplier (courier, agent). Not owed to the supplier.
    pub paid_to_others_cents: i64,
    /// What the supplier's bill comes to: the goods plus the extra charges it carries.
    pub total_cents: i64,
    /// The extra charges on this receipt, oldest first.
    pub charges: Vec<ReceiptCharge>,
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
}

/// A manager or owner's approval for the cash this receipt takes out of the drawer, goods payment
/// and extra-charge payments together. One approval covers the whole request.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptApproval {
    /// A manager or owner PIN.
    pub manager_pin: String,
    /// Why more is paid out than the drawer holds.
    #[serde(default)]
    pub override_reason: Option<String>,
}

/// What kind of extra charge this is. A discount lowers the cost; the rest raise it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChargeKind {
    Shipping,
    Customs,
    Handling,
    Insurance,
    Other,
    /// Taken off by the supplier (early payment, a price break). Counts as a negative charge.
    Discount,
}

impl ChargeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChargeKind::Shipping => "shipping",
            ChargeKind::Customs => "customs",
            ChargeKind::Handling => "handling",
            ChargeKind::Insurance => "insurance",
            ChargeKind::Other => "other",
            ChargeKind::Discount => "discount",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "shipping" => Some(ChargeKind::Shipping),
            "customs" => Some(ChargeKind::Customs),
            "handling" => Some(ChargeKind::Handling),
            "insurance" => Some(ChargeKind::Insurance),
            "other" => Some(ChargeKind::Other),
            "discount" => Some(ChargeKind::Discount),
            _ => None,
        }
    }
}

/// Who asks for the money of an extra charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BilledBy {
    /// It is on the supplier's bill: it adds to (or, for a discount, takes from) what is owed them.
    Supplier,
    /// Someone else is paid (courier, clearing agent, porter): paid now, not owed to the supplier.
    ThirdParty,
}

impl BilledBy {
    pub fn as_str(&self) -> &'static str {
        match self {
            BilledBy::Supplier => "supplier",
            BilledBy::ThirdParty => "third_party",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "supplier" => Some(BilledBy::Supplier),
            "third_party" => Some(BilledBy::ThirdParty),
            _ => None,
        }
    }
}

/// How an extra charge is spread over the products on the receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Allocation {
    /// In proportion to what each line costs.
    Value,
    /// In proportion to how many units each line has.
    Quantity,
}

impl Allocation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Allocation::Value => "value",
            Allocation::Quantity => "quantity",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "value" => Some(Allocation::Value),
            "quantity" => Some(Allocation::Quantity),
            _ => None,
        }
    }
}

/// One extra charge on a receipt being recorded.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptChargeRequest {
    pub kind: ChargeKind,
    /// Always positive; a discount is a `discount` kind, not a negative amount.
    pub amount_cents: i64,
    pub billed_by: BilledBy,
    /// How it is spread over the products. Required when the receipt has two or more lines,
    /// refused when it has one.
    #[serde(default)]
    pub allocation: Option<Allocation>,
    /// The saved payee (courier, agent) a third-party charge is paid to. Required for
    /// `third_party`, refused for `supplier`.
    #[serde(default)]
    pub payee_key: Option<String>,
    /// What an `other` charge is, in a few words (at most 80 characters). Optional, and refused
    /// for every other kind.
    #[serde(default)]
    pub description: Option<String>,
}

/// How the third-party extra charges are paid, as the receipt is recorded. Same sources as the
/// goods payment; the money is filed under the shop's "Stock charges" category.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChargesPayment {
    pub source: PaymentSource,
    /// This register's terminal. Required for the drawer and the owner's pocket, refused otherwise.
    #[serde(default)]
    pub terminal_id: Option<String>,
    /// The open shift the cash is booked on; omitted only between shifts.
    #[serde(default)]
    pub session_key: Option<String>,
    /// `YYYY-MM-DD` the money left the account. Required for the bank, a card and online,
    /// refused for the drawer and the owner's pocket.
    #[serde(default)]
    pub paid_on: Option<String>,
}

/// An extra charge as returned to API clients.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptCharge {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key (`rchg_...`).
    pub key: String,
    pub receipt_key: String,
    pub supplier_key: String,
    pub kind: ChargeKind,
    pub amount_cents: i64,
    pub billed_by: BilledBy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocation: Option<Allocation>,
    /// The payee a third-party charge was paid to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    /// What an `other` charge is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What paid a third-party charge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_kind: Option<PaymentKind>,
    /// Key of the cash movement or expense payment that paid a third-party charge.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_key: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

/// One product on a receipt being recorded.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptLineRequest {
    /// Foreign key of the product.
    pub product_key: String,
    /// Units received.
    pub quantity: i64,
    /// What the whole line costs in cents: the supplier's lot price, or quantity x price each.
    /// The unit cost is worked out from this (plus the line's share of extra charges), never sent.
    pub line_total_cents: i64,
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
    /// Extra charges: shipping, duty, handling, insurance, a discount. They go into the cost of the
    /// stock; the ones the supplier bills also change what is owed to the supplier.
    #[serde(default)]
    pub charges: Vec<ReceiptChargeRequest>,
    /// How the third-party charges are paid. Required when any charge is `third_party`, refused
    /// otherwise.
    #[serde(default)]
    pub charges_payment: Option<ChargesPayment>,
    /// Manager or owner approval for the drawer cash this request takes out.
    #[serde(default)]
    pub approval: Option<ReceiptApproval>,
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
