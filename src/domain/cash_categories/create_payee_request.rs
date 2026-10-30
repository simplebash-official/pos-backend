use super::payee_type::PayeeType;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /payees`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePayeeRequest {
    pub payee_type: PayeeType,
    pub name: String,
    #[serde(default)]
    pub phone: Option<String>,
}
