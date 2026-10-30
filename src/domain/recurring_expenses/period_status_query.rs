use serde::Deserialize;
use utoipa::IntoParams;

/// Query for `GET /recurring-expenses/period-status`.
#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PeriodStatusQuery {
    pub category_key: String,
    pub payee_key: Option<String>,
    /// `YYYY-MM`.
    pub period: String,
}
