use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /products`. `sort_by` is whitelisted against a
/// fixed set of fields before it ever reaches a Mongo `sort` document (see
/// `service::product::sort_field_for`) rather than passed through directly.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ProductListQuery {
    /// Free-text search — matches product name, SKU, or barcode, plus any
    /// product whose category or subcategory name contains the term.
    pub search: Option<String>,
    /// Filter products by category key.
    pub category_key: Option<String>,
    /// Filter products by subcategory key.
    pub subcategory_key: Option<String>,
    /// Filter products below their min stock threshold.
    pub low_stock: Option<bool>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page.
    pub limit: Option<u64>,
    /// Field to sort by.
    pub sort_by: Option<String>,
    /// Sort order ("asc" or "desc").
    pub sort_order: Option<String>,
}
