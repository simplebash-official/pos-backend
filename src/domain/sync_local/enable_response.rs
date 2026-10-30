use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct EnableResponse {
    pub device_id: String,
    /// Opening-balance stock movements created to make stock a pure ledger.
    pub opening_balances_created: i64,
    pub enqueued: i64,
}
