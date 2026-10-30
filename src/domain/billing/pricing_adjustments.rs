use serde::Deserialize;
use utoipa::ToSchema;

/// `discountType` must be `"percentage"` (`discountValue` is a 0–100
/// percent, e.g. `7.5`) or `"fixed"` (`discountValue` is a plain cents
/// amount) — `service::sale::complete_sale` computes the actual
/// `discountCents` from this against the resolved subtotal, and persists
/// both this type/value (source of truth) and the resulting cents on the
/// `Invoice`. Omitting `pricingAdjustments` entirely means no discount.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PricingAdjustments {
    /// Discount method ("percentage" or "fixed").
    pub discount_type: String,
    /// Discount numeric value (e.g. 10.0 for 10% or 500 for $5.00 fixed).
    pub discount_value: f64,
}
