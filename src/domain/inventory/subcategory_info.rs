use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// A subcategory as returned to API clients, persisted in its own
/// `subcategories` collection (see `modules::inventory::model::SubcategoryDocument`).
/// `category_key` is the FK back to its parent `CategoryInfo.key` — a
/// subcategory always belongs to exactly one category.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubcategoryInfo {
    /// Unique business key identifying the subcategory (e.g. sub_...).
    pub key: String,
    /// Foreign key referencing the parent category key.
    pub category_key: String,
    /// Subcategory display name.
    pub name: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last update timestamp.
    pub updated_at: DateTime<Utc>,
    /// Version counter.
    #[serde(default = "default_version")]
    pub version: i64,
    /// Deletion timestamp if soft-deleted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<DateTime<Utc>>,
    /// Device identifier that last updated this subcategory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
