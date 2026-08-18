// Mongo document shape for the purchase / stock-intake history feature.
// Kept separate from `domain::purchases` (the API-facing types) so BSON
// concerns like `ObjectId` never leak into request/response payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::purchases::{ProductSummary, Purchase, SupplierSummary};

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
    /// Cost price per unit in cents.
    pub unit_cost_cents: i64,
    /// Date when the stock purchase occurred.
    pub date: BsonDateTime,
    /// Supplier invoice or purchase order reference number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference_no: Option<String>,
    /// Optional remarks or notes regarding the purchase.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
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
        Purchase {
            id: self
                .id
                .expect("persisted purchase document must have an id")
                .to_hex(),
            key: self.key,
            supplier_key: self.supplier_key,
            product_key: self.product_key,
            quantity: self.quantity,
            unit_cost_cents: self.unit_cost_cents,
            total_cost_cents: self.quantity * self.unit_cost_cents,
            date: self.date.to_chrono(),
            reference_no: self.reference_no,
            notes: self.notes,
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
