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
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub key: String,
    pub supplier_key: String,
    pub product_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_price_cents: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
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
        }
    }
}
