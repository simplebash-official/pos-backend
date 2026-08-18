// Mongo document shape for the supplier-product linking feature. Kept
// separate from `domain::supplier_products` (the API-facing types) so BSON
// concerns like `ObjectId` never leak into request/response payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::domain::supplier_products::SupplierProductLink;

/// The join record between a `SupplierDocument` and an inventory
/// `ProductDocument`, referenced via their immutable `key`s
/// (`supplier_key`/`product_key`) rather than `ObjectId`s — same
/// cross-reference convention as `ProductDocument.category_key`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierProductLinkDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this link record (e.g. spl_...).
    pub key: String,
    /// Foreign key referencing the supplier.
    pub supplier_key: String,
    /// Foreign key referencing the product.
    pub product_key: String,
    /// Negotiated supplier-specific cost price in cents.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_price_cents: Option<i64>,
    /// Optional remarks or notes regarding this supplier link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Concurrency version counter for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when this link was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when this link was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Soft-delete timestamp, if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Device identifier that last updated this link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

impl SupplierProductLinkDocument {
    /// `created_at` doubles as the API-facing `addedAt` — a link has no
    /// separate "added" concept beyond when the document was first created.
    pub fn into_link(self) -> SupplierProductLink {
        SupplierProductLink {
            key: self.key,
            supplier_key: self.supplier_key,
            product_key: self.product_key,
            cost_price_cents: self.cost_price_cents,
            notes: self.notes,
            added_at: self.created_at.to_chrono(),
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }
}
