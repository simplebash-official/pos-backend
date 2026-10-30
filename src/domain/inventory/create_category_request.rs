use serde::Deserialize;
use utoipa::ToSchema;

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
