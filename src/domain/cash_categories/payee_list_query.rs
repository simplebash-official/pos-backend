use serde::Deserialize;
use utoipa::IntoParams;

/// Query for `GET /payees`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PayeeListQuery {
    pub payee_type: Option<String>,
    pub search: Option<String>,
    /// Also list hidden payees (needs `payees:manage`); the pickers never ask for them.
    pub include_hidden: Option<bool>,
}
