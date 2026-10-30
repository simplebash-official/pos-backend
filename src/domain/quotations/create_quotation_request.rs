use super::create_quotation_item::CreateQuotationItem;
use super::quotation_customer::QuotationCustomer;
use crate::domain::billing::PricingAdjustments;

use serde::Deserialize;
use utoipa::ToSchema;

/// Body for `POST /quotations`. Totals are never client-supplied — the server
/// recomputes every figure from the lines and `pricingAdjustments`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateQuotationRequest {
    #[serde(default)]
    pub customer: QuotationCustomer,
    pub items: Vec<CreateQuotationItem>,
    /// Order-level discount; omitted means none.
    #[serde(default)]
    pub pricing_adjustments: Option<PricingAdjustments>,
    /// Last day the offer holds, `YYYY-MM-DD`; today or later.
    pub valid_until: String,
    #[serde(default)]
    pub notes: Option<String>,
    /// Terms and conditions printed on the quotation.
    #[serde(default)]
    pub terms: Option<String>,
    /// Name of the person who prepared it (display only).
    #[serde(default)]
    pub prepared_by: Option<String>,
}
