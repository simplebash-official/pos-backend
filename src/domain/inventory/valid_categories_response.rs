use super::valid_category_option::ValidCategoryOption;

use serde::Serialize;
use utoipa::ToSchema;

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
