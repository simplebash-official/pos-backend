use super::credit_reason::CreditReason;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /supplier-credits`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateSupplierCreditRequest {
    pub supplier_key: String,
    /// The receipt the credit is against; omitted for a credit on the supplier's account.
    #[serde(default)]
    pub receipt_key: Option<String>,
    pub amount_cents: i64,
    pub reason: CreditReason,
    #[serde(default)]
    pub note: Option<String>,
}
