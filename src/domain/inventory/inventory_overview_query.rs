use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /overview`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct InventoryOverviewQuery {
    /// Free-text search — matches product name, SKU, or barcode, plus any
    /// product whose category or subcategory name contains the term.
    pub search: Option<String>,
    /// Filter by category key.
    pub category_key: Option<String>,
    /// Filter by subcategory key.
    pub subcategory_key: Option<String>,
    /// Filter only low stock items.
    pub low_stock: Option<bool>,
    /// Page number.
    pub page: Option<u64>,
    /// Page size.
    pub limit: Option<u64>,
    /// Sort field.
    pub sort_by: Option<String>,
    /// Sort direction.
    pub sort_order: Option<String>,
}
