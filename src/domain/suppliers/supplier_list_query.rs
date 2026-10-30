use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /suppliers`. `search` matches against name,
/// contact person, phone, and address; `category` filters by one of
/// `supplied_categories`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SupplierListQuery {
    /// Search term matching name, contact person, phone, or address.
    pub search: Option<String>,
    /// Filter suppliers by supplied category name.
    pub category: Option<String>,
}
