use serde::Deserialize;
use utoipa::ToSchema;

// Request and Response DTOs

/// Body for `POST /billing/sessions/open`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionRequest {
    pub terminal_id: String,
    pub opening_float_cents: i64,
    #[serde(default)]
    pub opening_denominations: Option<serde_json::Value>,
    #[serde(default)]
    pub opening_note: Option<String>,
    #[serde(default)]
    pub discrepancy_reason: Option<String>,
    #[serde(default)]
    pub manager_pin: Option<String>,
}
