use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /supplier-credits`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SupplierCreditListQuery {
    pub supplier_key: Option<String>,
    pub receipt_key: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}
