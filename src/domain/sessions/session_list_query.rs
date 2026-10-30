use serde::Deserialize;
use utoipa::IntoParams;

/// Query parameters for listing sessions.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct SessionListQuery {
    pub search: Option<String>,
    pub terminal_id: Option<String>,
    pub cashier_id: Option<String>,
    pub status: Option<String>,
    pub date_preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}
