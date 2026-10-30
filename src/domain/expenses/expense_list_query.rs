use serde::Deserialize;
use utoipa::IntoParams;

/// Query for `GET /expenses`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct ExpenseListQuery {
    pub search: Option<String>,
    pub category_key: Option<String>,
    pub payee_key: Option<String>,
    /// `unpaid` (anything with a balance), `overdue` or `paid`.
    pub status: Option<String>,
    /// `ledger` or `drawer`.
    pub source: Option<String>,
    /// `YYYY-MM-DD`, inclusive.
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}
