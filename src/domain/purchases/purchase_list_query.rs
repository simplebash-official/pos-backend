use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /purchases`. `supplierKey` and `productKey` are optional
/// to support full collection syncs.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PurchaseListQuery {
    /// Filter purchases by supplier key.
    pub supplier_key: Option<String>,
    /// Filter purchases by product key.
    pub product_key: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page limit.
    pub limit: Option<u64>,
}
