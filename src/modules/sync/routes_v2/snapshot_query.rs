use serde::Deserialize;
use utoipa::IntoParams;

/// Query of `GET /sync/snapshot`.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SnapshotQuery {
    /// Page token from the previous page (omit for the first page).
    pub page: Option<String>,
    /// Records per page (1..1000, default 500).
    pub limit: Option<i64>,
}
