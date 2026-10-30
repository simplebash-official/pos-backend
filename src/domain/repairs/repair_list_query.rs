use serde::Deserialize;
use utoipa::IntoParams;

/// Query params for `GET /repairs`. `search` matches ticket number,
/// customer name/phone, and device model; `status` filters to one exact
/// status value.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct RepairListQuery {
    /// Search term matching ticket number, customer name/phone, or model.
    pub search: Option<String>,
    /// Filter by status.
    pub status: Option<String>,
    /// Only `"today"` is meaningful — scopes to `created_at` within
    /// `shop_today_range()`. Any other value (or absence) means all time.
    pub date_preset: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Items per page limit.
    pub limit: Option<u64>,
}
