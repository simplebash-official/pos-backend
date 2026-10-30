use serde::Deserialize;
use utoipa::ToSchema;

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
