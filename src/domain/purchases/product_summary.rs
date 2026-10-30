use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Minimal product display data embedded on a `Purchase`, resolved live at
/// read time from `inventory::service::product::get_product_by_key` —
/// absent if the product no longer exists.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProductSummary {
    /// MongoDB hex ID.
    pub id: String,
    /// Product business key.
    pub key: String,
    /// SKU identifier code.
    pub sku: String,
    /// Product name.
    pub name: String,
    /// Parent category display name.
    pub category: String,
    /// Subcategory display name.
    pub subcategory: String,
}
