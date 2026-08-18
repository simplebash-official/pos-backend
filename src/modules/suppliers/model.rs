// Mongo document shape for the suppliers feature. Kept separate from
// `domain::suppliers` (the API-facing types) so BSON concerns like
// `ObjectId` never leak into request/response payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::suppliers::Supplier,
};

/// Mongo document shape representing a supplier record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this supplier (e.g. sup_...).
    #[serde(default)]
    pub key: String,
    /// Supplier company or trading name.
    pub name: String,
    /// Name of main point of contact at the supplier.
    pub contact_person: String,
    /// Primary contact telephone number.
    pub primary_phone: String,
    /// Secondary contact telephone number, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_phone: Option<String>,
    /// Physical or mailing address of the supplier.
    pub address: String,
    /// List of product categories supplied by this vendor.
    pub supplied_categories: Vec<String>,
    /// Email address of the supplier, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Internal notes or remarks regarding the supplier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Concurrency version counter for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when supplier record was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when supplier record was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Soft-delete timestamp, if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Device identifier that last updated this supplier record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

impl SupplierDocument {
    pub fn into_supplier(self) -> Supplier {
        let key = if self.key.is_empty() {
            generate_id(prefixes::SUPPLIER)
        } else {
            self.key
        };
        Supplier {
            id: self
                .id
                .expect("persisted supplier document must have an id")
                .to_hex(),
            key,
            name: self.name,
            contact_person: self.contact_person,
            primary_phone: self.primary_phone,
            secondary_phone: self.secondary_phone,
            address: self.address,
            supplied_categories: self.supplied_categories,
            email: self.email,
            notes: self.notes,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }
}
