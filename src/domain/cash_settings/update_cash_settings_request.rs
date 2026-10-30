use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `PUT /cash-settings`. The whole set is sent every time; an absent optional value clears it.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCashSettingsRequest {
    pub undo_window_seconds: i64,
    #[serde(default)]
    pub approval_threshold_cents: Option<i64>,
    pub quick_amounts_cents: Vec<i64>,
    #[serde(default)]
    pub float_target_cents: Option<i64>,
}
