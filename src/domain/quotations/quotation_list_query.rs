use serde::Deserialize;
use utoipa::IntoParams;

/// Query parameters for `GET /quotations`.
#[derive(Debug, Clone, Default, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct QuotationListQuery {
    /// Keyword matching quotation number or customer name/phone.
    pub search: Option<String>,
    /// `active` (still valid), `expired`, or `cancelled`. Omitted = all.
    pub status: Option<String>,
    /// Page number (1-indexed).
    pub page: Option<u64>,
    /// Page size.
    pub limit: Option<u64>,
}
