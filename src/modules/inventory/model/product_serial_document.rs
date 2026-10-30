use super::default_version::default_version;
use crate::core::constants::prefixes;
use crate::core::id::generate_id;
use crate::domain::inventory::{ProductSerial, SerialStatus};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Mongo document shape for one physical serialized unit of a product,
/// persisted in its own `product_serials` collection (never embedded on
/// `ProductDocument` — a serialized product can have thousands of units,
/// each with its own independent lifecycle, so this is a one-to-many child
/// collection referencing the product by `product_key`, the same pattern
/// `StockMovementDocument` uses).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductSerialDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this serial record (e.g. psn_...).
    #[serde(default)]
    pub key: String,
    /// Foreign key referencing the parent product.
    pub product_key: String,
    /// The physical serial number, unique across all products (enforced by
    /// a unique Mongo index — see `clients::indexes`).
    pub serial_number: String,
    /// Key of the stock batch this unit was received in.
    pub batch_key: String,
    /// Current lifecycle status of this unit.
    pub status: SerialStatus,
    /// Key of the invoice this unit was sold on, once `Sold`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoice_key: Option<String>,
    /// When this unit was sold, once `Sold`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sold_at: Option<BsonDateTime>,
    /// Warranty length snapshotted from the product at the moment of sale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_months: Option<i64>,
    /// Computed `sold_at + warranty_months`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warranty_expires_at: Option<BsonDateTime>,
    /// Key of the credit note that most recently returned this unit, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_note_key: Option<String>,
    /// Concurrency version counter for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when this serial record was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when this serial record was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
}

impl ProductSerialDocument {
    #[must_use]
    pub fn into_product_serial(self) -> ProductSerial {
        let key = if self.key.is_empty() {
            generate_id(prefixes::PRODUCT_SERIAL)
        } else {
            self.key
        };
        ProductSerial {
            id: self
                .id
                .expect("persisted product serial document must have an id")
                .to_hex(),
            key,
            product_key: self.product_key,
            serial_number: self.serial_number,
            batch_key: self.batch_key,
            status: self.status,
            invoice_key: self.invoice_key,
            sold_at: self.sold_at.map(|d| d.to_chrono()),
            warranty_months: self.warranty_months,
            warranty_expires_at: self.warranty_expires_at.map(|d| d.to_chrono()),
            credit_note_key: self.credit_note_key,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
        }
    }
}
