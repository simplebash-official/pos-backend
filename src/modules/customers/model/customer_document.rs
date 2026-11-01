use crate::core::constants::prefixes;
use crate::core::id::generate_id;
use crate::domain::customers::Customer;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// Mongo document shape representing a customer record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomerDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this customer (e.g. cus_...).
    pub key: String,
    /// Customer's full name or business name.
    pub name: String,
    /// Name of the primary contact person for corporate/business accounts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_person: Option<String>,
    /// Primary telephone or mobile number.
    pub primary_phone: String,
    /// Secondary or alternate phone number.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_phone: Option<String>,
    /// Email address of the customer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Physical or billing address of the customer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// Descriptive labels or categorization tags (e.g. "VIP", "Wholesale").
    pub tags: Vec<String>,
    /// Internal notes or remarks regarding this customer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Total outstanding unpaid balance in cents.
    pub outstanding_balance_cents: i64,
    /// Cumulative lifetime total purchases in cents.
    pub total_purchases_cents: i64,
    /// Concurrency version counter for optimistic locking.
    pub version: i64,
    /// Timestamp when this customer profile was created.
    pub created_at: BsonDateTime,
    /// Timestamp when this customer profile was last updated.
    pub updated_at: BsonDateTime,
    /// Timestamp when this customer was soft-deleted, if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Identifier of the device or sync client that last updated this document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl CustomerDocument {
    #[must_use]
    /// Converts the stored document into the type the API returns.
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
