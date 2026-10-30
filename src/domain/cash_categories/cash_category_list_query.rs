use serde::Deserialize;
use utoipa::IntoParams;

/// Query for `GET /cash-categories`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct CashCategoryListQuery {
    pub movement_type: Option<String>,
    /// Hidden categories are left out unless this is true (the management screen).
    pub include_hidden: Option<bool>,
    /// Matches name or Sinhala name, ignoring case and spacing.
    pub search: Option<String>,
    /// Only categories waiting for a manager's review.
    pub needs_review: Option<bool>,
    /// Only categories that count as a business cost, for the expense ledger.
    pub for_expenses: Option<bool>,
    /// Only categories that count as income, for the income ledger.
    pub for_income: Option<bool>,
}
