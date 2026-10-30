use serde::Serialize;
use utoipa::ToSchema;

/// One subcategory option nested under `ValidCategoryOption`.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ValidSubcategoryOption {
    /// Subcategory key.
    pub key: String,
    /// Subcategory name.
    pub name: String,
}
