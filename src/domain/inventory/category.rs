//! Categories and subcategories with their requests and responses.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Body for `POST /categories/{categoryKey}/subcategories` (admin-only).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AddSubcategoryRequest {
    /// Name of the new subcategory to add.
    pub name: String,
}

/// Response for `GET /categories`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CategoriesResponse {
    /// List of all categories with their subcategories.
    pub categories: Vec<CategoryInfo>,
}

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

/// Body for `POST /categories` (admin-only — see `core::middleware::auth::AdminUser`).
/// `subcategories` is a bulk list of subcategory *names* — each becomes its
/// own `SubcategoryDocument` (with its own generated key) referencing the
/// newly created category, so a category can be bootstrapped with its
/// starter subcategories in one call.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateCategoryRequest {
    /// Display name of the new category.
    pub name: String,
    /// Icon name or identifier.
    pub icon: String,
    /// UI theme color code.
    pub color: String,
    /// Initial list of subcategory names to create under this category.
    #[serde(default)]
    pub subcategories: Vec<String>,
}

/// Response for `GET /categories/{categoryKey}/subcategories`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubcategoriesResponse {
    /// Parent category key.
    pub category_key: String,
    /// Subcategories under the parent category.
    pub subcategories: Vec<SubcategoryInfo>,
}

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

/// Body for `PUT /categories/{categoryKey}` (admin-only). All fields
/// optional, same partial-update convention as `UpdateProductRequest`.
/// Renaming (`name` set to a new value) is a pure display-label change —
/// products reference a category by its immutable `key`, so no cascade is
/// needed (see `service::category::update_category`).
#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct UpdateCategoryRequest {
    /// Updated display name.
    pub name: Option<String>,
    /// Updated icon name.
    pub icon: Option<String>,
    /// Updated theme color code.
    pub color: Option<String>,
}

/// Response for `GET /categories/valid` — the same category/subcategory
/// data as `CategoriesResponse`, reshaped as key+name pairs for callers
/// (like a product form) that need to submit `categoryKey`/`subcategoryKey`
/// while displaying the human-readable name.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidCategoriesResponse {
    /// List of category and subcategory selection options.
    pub categories: Vec<ValidCategoryOption>,
}

/// One category option within `ValidCategoriesResponse` — key + display
/// name plus its own nested subcategory options, so a client (e.g. a
/// product-creation form) can render names while submitting keys.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidCategoryOption {
    /// Category key.
    pub key: String,
    /// Category name.
    pub name: String,
    /// Subcategory selection options.
    pub subcategories: Vec<ValidSubcategoryOption>,
}

/// One subcategory option nested under `ValidCategoryOption`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidSubcategoryOption {
    /// Subcategory key.
    pub key: String,
    /// Subcategory name.
    pub name: String,
}
