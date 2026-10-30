use super::default_version::default_version;
use crate::core::constants::prefixes;
use crate::core::id::generate_id;
use crate::domain::inventory::SubcategoryInfo;
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// A subcategory, persisted in its own `subcategories` collection rather
/// than embedded on `CategoryDocument` — this gives every subcategory its
/// own stable `key` (so products can reference it directly, and duplicate
/// names are only disallowed within the same category, not globally) and
/// lets a category be renamed without touching any subcategory or product
/// document. `category_key` is the FK back to `CategoryDocument.key`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubcategoryDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this subcategory (e.g. sub_...).
    #[serde(default)]
    pub key: String,
    /// Foreign key referencing the parent category key.
    pub category_key: String,
    /// Subcategory display name.
    pub name: String,
    /// Concurrency version counter for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when subcategory was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when subcategory was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Soft-delete timestamp, if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Device identifier that last updated this subcategory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl SubcategoryDocument {
    #[must_use]
    pub fn into_subcategory_info(self) -> SubcategoryInfo {
        let key = if self.key.is_empty() {
            generate_id(prefixes::SUBCATEGORY)
        } else {
            self.key
        };
        SubcategoryInfo {
            key,
            category_key: self.category_key,
            name: self.name,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }
}
