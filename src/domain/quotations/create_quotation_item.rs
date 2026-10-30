use serde::Deserialize;
use utoipa::ToSchema;

/// One requested line. With a `productKey` the server takes name and SKU from
/// the catalog and the price from `sellingPriceCents`, which must be one of
/// the product's current prices (`Product.price_tiers`); without one the line
/// is ad hoc and `name` + `unitPriceCents` are required.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateQuotationItem {
    #[serde(default)]
    pub product_key: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub unit_price_cents: Option<i64>,
    /// For a product line: the price picked from the product's current prices.
    #[serde(default)]
    pub selling_price_cents: Option<i64>,
    /// Units offered; must be at least 1.
    pub quantity: i64,
    /// Discount on this line in cents (cannot exceed the line's gross value).
    #[serde(default)]
    pub discount_cents: i64,
}
