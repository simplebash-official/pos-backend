use serde::Deserialize;
use utoipa::IntoParams;

/// The days a report covers, both inclusive.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
pub(super) struct PeriodQuery {
    /// First day, `YYYY-MM-DD`.
    pub(super) from: Option<String>,
    /// Last day, `YYYY-MM-DD`.
    pub(super) to: Option<String>,
}
