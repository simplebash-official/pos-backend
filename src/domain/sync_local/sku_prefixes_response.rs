use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response of `GET /api/sync/sku-prefixes`: the distinct SKU block families
/// (derived category+subcategory codes, e.g. "PHO-SCR") this device's local
/// catalog currently needs a cloud-reserved number block for.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SkuPrefixesResponse {
    pub prefixes: Vec<String>,
}
