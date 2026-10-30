use serde::Deserialize;
use utoipa::IntoParams;

/// Query parameters for listing cash movements.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct CashMovementListQuery {
    pub session_key: Option<String>,
    pub terminal_id: Option<String>,
    pub movement_type: Option<String>,
    pub category_key: Option<String>,
    /// The category plus everything merged into it; filled in by the service from `category_key`.
    #[serde(skip)]
    #[param(ignore)]
    pub category_keys: Option<Vec<String>>,
    pub payee_key: Option<String>,
    pub recorded_late: Option<bool>,
    pub date_preset: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<u64>,
    pub limit: Option<u64>,
}
