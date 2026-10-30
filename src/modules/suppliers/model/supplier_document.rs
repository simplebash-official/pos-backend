use crate::core::constants::prefixes;
use crate::core::id::generate_id;
use crate::domain::suppliers::{ContactPerson, Supplier};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

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
    /// All contact persons belonging to this supplier, each with up to 2 phone numbers.
    #[serde(default)]
    pub contact_persons: Vec<ContactPerson>,
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
    #[must_use]
    /// Converts the stored document into the type the API returns.
    pub fn into_supplier(self) -> Supplier {
        let key = if self.key.is_empty() {
            generate_id(prefixes::SUPPLIER)
        } else {
            self.key
        };
        let contact_persons = if !self.contact_persons.is_empty() {
            self.contact_persons
        } else if !self.contact_person.is_empty() && !self.primary_phone.is_empty() {
            let mut phones = vec![self.primary_phone.clone()];
            if let Some(ref sec) = self.secondary_phone
                && !sec.is_empty()
            {
                phones.push(sec.clone());
            }
            vec![ContactPerson {
                name: self.contact_person.clone(),
                phone_numbers: phones,
            }]
        } else {
            Vec::new()
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
            contact_persons,
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
