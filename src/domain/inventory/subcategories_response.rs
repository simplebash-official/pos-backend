use super::subcategory_info::SubcategoryInfo;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /categories/{categoryKey}/subcategories`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubcategoriesResponse {
    /// Parent category key.
    pub category_key: String,
    /// Subcategories under the parent category.
    pub subcategories: Vec<SubcategoryInfo>,
}
