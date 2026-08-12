// Mongo document shape for the customers feature. Kept separate from
// `domain::customers` (the API-facing types) so BSON concerns like
// `ObjectId` never leak into request/response payloads.

use mongodb::bson::{DateTime as BsonDateTime, oid::ObjectId};
use serde::{Deserialize, Serialize};

use crate::{
    core::{constants::prefixes, id::generate_id},
    domain::customers::Customer,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerDocument {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default)]
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_person: Option<String>,
    pub primary_phone: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub outstanding_balance_cents: i64,
    #[serde(default)]
    pub total_purchases_cents: i64,
    #[serde(default = "default_version")]
    pub version: i64,
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}

impl CustomerDocument {
    pub fn into_customer(self) -> Customer {
        let key = if self.key.is_empty() {
            generate_id(prefixes::CUSTOMER)
        } else {
            self.key
        };
        Customer {
            id: self
                .id
                .expect("persisted customer document must have an id")
                .to_hex(),
            key,
            name: self.name,
            contact_person: self.contact_person,
            primary_phone: self.primary_phone,
            secondary_phone: self.secondary_phone,
            email: self.email,
            address: self.address,
            tags: self.tags,
            notes: self.notes,
            outstanding_balance_cents: self.outstanding_balance_cents,
            total_purchases_cents: self.total_purchases_cents,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }
}
