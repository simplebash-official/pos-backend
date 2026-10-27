// Mongo document shape for the purchase / stock-intake history feature.
// Kept separate from `domain::purchases` (the API-facing types) so BSON
// concerns like `ObjectId` never leak into request/response payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::{
    purchase_receipts::{
        Allocation, BilledBy, ChargeKind, PaymentKind, PurchaseReceipt, ReceiptCharge,
        ReceiptSettlement, ReceiptStatus,
    },
    purchases::{ProductSummary, Purchase, SupplierSummary},
    supplier_credits::{CreditReason, SupplierCredit},
};
use crate::modules::purchases::charges;

/// A single stock-intake record. References `suppliers`/inventory
/// `products` via their immutable `key`s (`supplier_key`/`product_key`),
/// never a raw `ObjectId` — same cross-reference convention as
/// `ProductDocument.category_key`. `total_cost_cents` is deliberately not
/// stored (see `domain::purchases::Purchase`'s doc comment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this purchase record (e.g. pur_...).
    pub key: String,
    /// Foreign key referencing the supplier.
    pub supplier_key: String,
    /// Foreign key referencing the purchased product.
    pub product_key: String,
    /// Number of product units received.
    pub quantity: i64,
    /// What the whole line cost in cents (the supplier's lot price). The source of truth for the
    /// line's cost; the unit cost is derived from it so a lot that does not divide evenly stays exact.
    pub line_total_cents: i64,
    /// This line's share of the receipt's extra charges in cents (discounts negative), fixed when
    /// the receipt was recorded. The stock batches were costed from `line_total + share`.
    #[serde(default)]
    pub charges_share_cents: i64,
    /// Selling price per unit set at receipt, in cents.
    pub selling_price_cents: i64,
    /// Date when the stock purchase occurred.
    pub date: BsonDateTime,
    /// Foreign key referencing the receipt (supplier invoice) this line belongs to.
    pub receipt_key: String,
    /// Concurrency version counter for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when purchase document was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when purchase document was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Soft-delete timestamp, if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Device identifier that last updated this purchase record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

impl PurchaseDocument {
    /// `supplier`/`product` are resolved by the service layer (a lookup by
    /// `supplier_key`/`product_key`, `None` if the reference no longer
    /// resolves) before calling this — this conversion stays a pure/sync
    /// mapping with no DB access, matching every other `into_*` conversion
    /// in this codebase.
    pub fn into_purchase(
        self,
        supplier: Option<SupplierSummary>,
        product: Option<ProductSummary>,
    ) -> Purchase {
        let landed = self.line_total_cents + self.charges_share_cents;
        Purchase {
            id: self
                .id
                .expect("persisted purchase document must have an id")
                .to_hex(),
            key: self.key,
            supplier_key: self.supplier_key,
            product_key: self.product_key,
            quantity: self.quantity,
            line_total_cents: self.line_total_cents,
            charges_share_cents: self.charges_share_cents,
            landed_total_cents: landed,
            landed_unit_cost_cents: charges::rounded_unit(landed, self.quantity),
            selling_price_cents: self.selling_price_cents,
            date: self.date.to_chrono(),
            receipt_key: self.receipt_key,
            supplier,
            product,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }
}

/// The header of one supplier invoice: who it came from, when, and the supplier's own invoice
/// number. Immutable once saved. Its lines are `PurchaseDocument`s pointing back at `key`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurchaseReceiptDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key (`prc_...`).
    pub key: String,
    /// Running number shown to people (`PUR-000001`).
    pub receipt_number: String,
    /// Foreign key referencing the supplier.
    pub supplier_key: String,
    /// The supplier's invoice or delivery-note number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    /// Day the stock was received.
    pub date: BsonDateTime,
    /// Optional remarks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

/// What the service works out about a receipt from the records that point at it.
pub struct ReceiptFigures {
    /// Taken off the bill by supplier credits.
    pub credited_cents: i64,
    /// The payments counted against the receipt, reversed ones included (marked as such).
    pub settlements: Vec<ReceiptSettlement>,
    /// The extra charges on the receipt.
    pub charges: Vec<ReceiptCharge>,
}

impl PurchaseReceiptDocument {
    /// The supplier, the lines and the figures are resolved by the service layer; this stays a
    /// pure mapping. What is paid, what is due and the status follow from them.
    pub fn into_receipt(
        self,
        supplier: Option<SupplierSummary>,
        lines: Vec<Purchase>,
        figures: ReceiptFigures,
    ) -> PurchaseReceipt {
        let goods_cents: i64 = lines.iter().map(|line| line.line_total_cents).sum();
        let totals = charges::charge_totals(
            &figures
                .charges
                .iter()
                .map(|charge| charges::ChargeFigure {
                    kind: charge.kind,
                    amount_cents: charge.amount_cents,
                    billed_by: charge.billed_by,
                })
                .collect::<Vec<_>>(),
        );
        let total_cents = charges::owed_total(goods_cents, totals);
        let paid_cents: i64 = figures
            .settlements
            .iter()
            .filter(|s| !s.reversed)
            .map(|s| s.amount_cents)
            .sum();
        let due_cents = total_cents - figures.credited_cents - paid_cents;
        PurchaseReceipt {
            id: self
                .id
                .expect("persisted receipt document must have an id")
                .to_hex(),
            key: self.key,
            receipt_number: self.receipt_number,
            supplier_key: self.supplier_key,
            reference_no: self.reference_no,
            date: self.date.to_chrono(),
            notes: self.notes,
            supplier,
            goods_cents,
            extra_charges_cents: totals.supplier_billed_cents,
            paid_to_others_cents: totals.paid_to_others_cents,
            total_cents,
            charges: figures.charges,
            credited_cents: figures.credited_cents,
            paid_cents,
            due_cents,
            status: ReceiptStatus::of(due_cents, figures.credited_cents + paid_cents),
            settlements: figures.settlements,
            lines,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            updated_by_device: self.updated_by_device,
        }
    }
}

/// One payment, or part of one, counted against a receipt. Immutable: a payment that is
/// reversed is not edited here, the settlement stops counting because its payment was reversed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptSettlementDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key (`rset_...`).
    pub key: String,
    /// Foreign key referencing the receipt that was paid.
    pub receipt_key: String,
    /// Foreign key referencing the supplier the receipt belongs to.
    pub supplier_key: String,
    pub amount_cents: i64,
    pub payment_kind: PaymentKind,
    /// Key of the cash movement or expense payment that paid.
    pub payment_key: String,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl ReceiptSettlementDocument {
    /// `reversed` is worked out by the service from the payment's own reversal.
    pub fn into_settlement(self, reversed: bool) -> ReceiptSettlement {
        ReceiptSettlement {
            id: self
                .id
                .expect("persisted settlement document must have an id")
                .to_hex(),
            key: self.key,
            receipt_key: self.receipt_key,
            supplier_key: self.supplier_key,
            amount_cents: self.amount_cents,
            payment_kind: self.payment_kind,
            payment_key: self.payment_key,
            reversed,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}

/// A credit a supplier gave: an amount taken off what the shop owes. Immutable; a wrong credit
/// is voided by a reversing entry (`reverses_key`) whose key is derived from the original's.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierCreditDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key (`scr_...`).
    pub key: String,
    pub supplier_key: String,
    /// The receipt this credit is against, when it is against one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt_key: Option<String>,
    pub amount_cents: i64,
    pub reason: CreditReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Set on a voiding entry: the credit it cancels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverses_key: Option<String>,
    pub recorded_by_user_id: String,
    pub recorded_by_name_snapshot: String,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl SupplierCreditDocument {
    /// `voided_by_key` is worked out by the service from the voiding entry.
    pub fn into_credit(self, voided_by_key: Option<String>) -> SupplierCredit {
        SupplierCredit {
            id: self
                .id
                .expect("persisted credit document must have an id")
                .to_hex(),
            key: self.key,
            supplier_key: self.supplier_key,
            receipt_key: self.receipt_key,
            amount_cents: self.amount_cents,
            reason: self.reason,
            note: self.note,
            reverses_key: self.reverses_key,
            voided_by_key,
            recorded_by_user_id: self.recorded_by_user_id,
            recorded_by_name_snapshot: self.recorded_by_name_snapshot,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}

/// One extra charge on a receipt (shipping, duty, handling, insurance, other, or a supplier
/// discount). Immutable. A third-party charge also records the payment that paid it; a charge the
/// supplier billed is settled with the rest of the bill, so it carries no payment of its own.
/// `supplier_key` and `date` repeat the receipt's so the supplier balance can add a supplier's
/// charges up without loading every receipt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptChargeDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key (`rchg_...`).
    pub key: String,
    /// Foreign key referencing the receipt the charge is on.
    pub receipt_key: String,
    /// Foreign key referencing the receipt's supplier.
    pub supplier_key: String,
    /// The receipt's day.
    pub date: BsonDateTime,
    pub kind: ChargeKind,
    /// Always positive; the sign comes from `kind`.
    pub amount_cents: i64,
    pub billed_by: BilledBy,
    /// How the charge was spread over the lines; absent for a single-line receipt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocation: Option<Allocation>,
    /// The payee a third-party charge was paid to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee_key: Option<String>,
    /// What an `other` charge is; absent for every other kind and for old rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_kind: Option<PaymentKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_key: Option<String>,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl ReceiptChargeDocument {
    pub fn into_charge(self) -> ReceiptCharge {
        ReceiptCharge {
            id: self
                .id
                .expect("persisted charge document must have an id")
                .to_hex(),
            key: self.key,
            receipt_key: self.receipt_key,
            supplier_key: self.supplier_key,
            kind: self.kind,
            amount_cents: self.amount_cents,
            billed_by: self.billed_by,
            allocation: self.allocation,
            payee_key: self.payee_key,
            description: self.description,
            payment_kind: self.payment_kind,
            payment_key: self.payment_key,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}
