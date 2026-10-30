use serde::Deserialize;
use utoipa::IntoParams;

/// Query parameters for listing and filtering print jobs.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PrintJobListQuery {
    /// Keyword search matching ticket number or customer name/phone.
    pub search: Option<String>,
    /// Filter by status.
    pub status: Option<String>,
    /// Only `"today"` is meaningful — scopes to `created_at` within
    /// `shop_today_range()`. Any other value (or absence) means all time.
    pub date_preset: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Page items limit.
    pub limit: Option<u64>,
}
