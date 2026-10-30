use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /billing/sessions/{key}/blind-count`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubmitBlindCountRequest {
    pub counted_cash_cents: i64,
    #[serde(default)]
    pub closing_denominations: Option<serde_json::Value>,
}
