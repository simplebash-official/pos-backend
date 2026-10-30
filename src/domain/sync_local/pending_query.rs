use serde::Deserialize;
use utoipa::IntoParams;

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PendingQuery {
    /// Only this resource (wire name, e.g. `invoices`).
    pub resource: Option<String>,
    /// Page size, 1..200 (default 50).
    pub limit: Option<i64>,
}
