// Mongo document shape for the suppliers feature. Kept separate from
// `domain::suppliers` (the API-facing types) so BSON concerns like
// `ObjectId` never leak into request/response payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::suppliers::Supplier,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplierDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub name: String,
    pub contact_person: String,
    pub primary_phone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_phone: Option<String>,
    pub address: String,
    pub supplied_categories: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
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
        }
    }
}
