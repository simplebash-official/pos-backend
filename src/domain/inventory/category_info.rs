use super::subcategory_info::SubcategoryInfo;

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

/// A main category and its subcategories, as returned to API clients.
/// `subcategories` is the authoritative allow-list — a product's
/// `subcategory_key` field is validated against it on create/update (see
/// `service::product::ensure_valid_category`).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInfo {
    /// Unique business key identifying the category (e.g. cat_...).
    pub key: String,
    /// Category display name.
    pub name: String,
    /// Icon name/identifier.
    pub icon: String,
    /// Category color hex code or CSS color.
    pub color: String,
    /// Nested list of subcategories belonging to this category.
    pub subcategories: Vec<SubcategoryInfo>,
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
    /// Device identifier that last updated this category.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by_device: Option<String>,
}
