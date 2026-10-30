use super::default_version::default_version;

use crate::domain::purchases::{ProductSummary, Purchase, SupplierSummary};

use crate::modules::purchases::charges;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

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
