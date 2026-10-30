use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /purchase-receipts`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ReceiptListQuery {
    /// Only this supplier's receipts.
    pub supplier_key: Option<String>,
    /// Only receipts with something still due (`true`), or only settled ones (`false`).
    pub open: Option<bool>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page limit.
    pub limit: Option<u64>,
}
