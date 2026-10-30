use super::default_version::default_version;
use super::receipt_figures::ReceiptFigures;
use crate::domain::purchase_receipts::{PurchaseReceipt, ReceiptStatus};
use crate::domain::purchases::{Purchase, SupplierSummary};

use crate::modules::purchases::charges;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

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
