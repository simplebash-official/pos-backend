use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /categories/{categoryKey}/subcategories` (admin-only).
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AddSubcategoryRequest {
    /// Name of the new subcategory to add.
    pub name: String,
}
