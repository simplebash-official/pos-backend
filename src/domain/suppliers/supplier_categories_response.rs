use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /suppliers/categories` — every distinct tag currently
/// in use across all suppliers' `supplied_categories`, for a client-side
/// autocomplete (replacing the frontend's hardcoded suggestion list).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierCategoriesResponse {
    /// Unique list of category tags across all suppliers.
    pub categories: Vec<String>,
}
