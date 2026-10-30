use serde::Deserialize;
use utoipa::IntoParams;

/// Query for `GET /recurring-expenses`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct RecurringExpenseListQuery {
    /// Stopped templates are left out unless this is true.
    pub include_inactive: Option<bool>,
}
