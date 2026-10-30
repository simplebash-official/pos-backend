use serde::Deserialize;
use utoipa::IntoParams;

#[derive(Debug, Clone, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct OutboxQuery {
    /// Return outbox rows with a sequence number greater than this (default 0).
    pub after: Option<i64>,
    /// Page size, 1..500 (default 200).
    pub limit: Option<i64>,
}
