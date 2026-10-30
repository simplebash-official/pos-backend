use serde::Serialize;
use utoipa::ToSchema;

/// A supplier the shop owes money to.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PayableSupplier {
    pub supplier_key: String,
    pub name: String,
    pub balance_cents: i64,
}
