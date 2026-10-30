use serde::Deserialize;
use utoipa::IntoParams;

/// Query of `GET /sync/pull`.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PullQuery {
    /// Last `seq` the device has applied (0 for a fresh device).
    pub since: Option<i64>,
    /// Page size (1..500, default 200).
    pub limit: Option<i64>,
}
