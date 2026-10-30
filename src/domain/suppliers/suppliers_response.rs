use super::supplier::Supplier;

use serde::Serialize;
use utoipa::ToSchema;

/// Response for `GET /suppliers`. Unlike `inventory`'s product list, this
/// isn't paginated — a shop's supplier list is small enough to return in
/// full, matching the frontend contract (no page/limit params in the spec).
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SuppliersResponse {
    /// Complete list of suppliers.
    pub suppliers: Vec<Supplier>,
}
