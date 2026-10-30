use super::default_version::default_version;
use crate::core::constants::prefixes;
use crate::core::id::generate_id;
use crate::domain::inventory::{CategoryInfo, SubcategoryInfo};
use mongodb::bson::DateTime as BsonDateTime;
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// A main category, persisted in the `categories` collection so the valid
/// set is a single DB-backed source of truth (for both API responses and
/// product validation) instead of a static table baked into the binary.
/// Its subcategories are NOT embedded here — they're independent documents
/// in the `subcategories` collection (see `SubcategoryDocument`), each
/// referencing this category by `key` so a category rename never needs to
/// cascade anywhere. Seeded via `cargo run --bin seed_providers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryDocument {
    /// MongoDB internal document identifier.
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Unique business key identifying this category (e.g. cat_...).
    #[serde(default)]
    pub key: String,
    /// Category display name.
    pub name: String,
    /// Icon identifier or name for UI display.
    pub icon: String,
    /// Color hex code or theme token for UI badges.
    pub color: String,
    /// Concurrency version counter for optimistic locking.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Timestamp when category was created.
    #[serde(default = "BsonDateTime::now")]
    pub created_at: BsonDateTime,
    /// Timestamp when category was last updated.
    #[serde(default = "BsonDateTime::now")]
    pub updated_at: BsonDateTime,
    /// Soft-delete timestamp, if deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<BsonDateTime>,
    /// Device identifier that last updated this category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}

impl CategoryDocument {
    /// `subcategories` is resolved by the service layer (a query against
    /// the `subcategories` collection filtered by this category's `key`)
    /// before calling this — see the comment on `ProductDocument::into_product`.
    #[must_use]
    pub fn into_category_info(self, subcategories: Vec<SubcategoryInfo>) -> CategoryInfo {
        let key = if self.key.is_empty() {
            generate_id(prefixes::CATEGORY)
        } else {
            self.key
        };
        CategoryInfo {
            key,
            name: self.name,
            icon: self.icon,
            color: self.color,
            subcategories,
            created_at: self.created_at.to_chrono(),
            updated_at: self.updated_at.to_chrono(),
            version: self.version,
            deleted_at: self.deleted_at.map(|d| d.to_chrono()),
            updated_by_device: self.updated_by_device,
        }
    }
}
