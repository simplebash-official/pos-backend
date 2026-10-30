use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /billing/sessions/{key}/close`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CloseSessionRequest {
    pub counted_cash_cents: i64,
    #[serde(default)]
    pub closing_denominations: Option<serde_json::Value>,
    pub closing_float_cents: i64,
    #[serde(default)]
    pub cash_drop_to_safe_cents: Option<i64>,
    #[serde(default)]
    pub closing_note: Option<String>,
    #[serde(default)]
    pub variance_reason: Option<String>,
    #[serde(default)]
    pub manager_pin: Option<String>,
}
