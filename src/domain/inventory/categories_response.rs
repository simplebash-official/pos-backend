use super::category_info::CategoryInfo;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /categories`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CategoriesResponse {
    /// List of all categories with their subcategories.
    pub categories: Vec<CategoryInfo>,
}
