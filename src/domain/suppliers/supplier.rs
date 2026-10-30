use super::contact_person::ContactPerson;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A supplier as returned to API clients. `id` (the Mongo `ObjectId` as a
/// hex string) is the stable route/lookup key (`GET/PUT/PATCH/DELETE
/// /suppliers/{id}`); `key` is the human-shareable prefixed id (see
/// `core::id::generate_id`), used by other modules (`supplier_products`,
/// `purchases`) to reference this supplier without holding its `ObjectId`.
/// `supplied_categories` are free-text tags (e.g. "Phone Parts") describing
/// what a supplier deals in — unlike `category_key` on a product, these are
/// not a foreign key into any collection, just descriptive labels a
/// supplier's own record carries.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Supplier {
    /// MongoDB hex ID.
    pub id: String,
    /// Unique business key identifying this supplier (e.g. sup_...).
    pub key: String,
    /// Supplier company or vendor name.
    pub name: String,
    /// Primary contact person name (kept in sync with primary contact person).
    pub contact_person: String,
    /// Primary telephone number (kept in sync with primary contact person).
    pub primary_phone: String,
    /// Secondary phone number if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_phone: Option<String>,
    /// All contact persons belonging to this supplier, each with up to 2 phone numbers.
    #[serde(default)]
    pub contact_persons: Vec<ContactPerson>,
    /// Physical or postal address.
    pub address: String,
    /// Categories of products supplied by this vendor.
    pub supplied_categories: Vec<String>,
    /// Email address of the supplier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// General notes or remarks.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Timestamp when supplier was created.
    pub created_at: DateTime<Utc>,
    /// Timestamp when supplier was last updated.
    pub updated_at: DateTime<Utc>,
    /// Optimistic locking version number.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

fn default_version() -> i64 {
    1
}
