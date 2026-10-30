use super::valid_subcategory_option::ValidSubcategoryOption;

use serde::Serialize;
use utoipa::ToSchema;

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
