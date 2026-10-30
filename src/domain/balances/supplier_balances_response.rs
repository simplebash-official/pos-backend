use super::supplier_balance::SupplierBalance;

use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SupplierBalancesResponse {
    pub suppliers: Vec<SupplierBalance>,
}
