use serde::Serialize;
use utoipa::ToSchema;

/// A product selling today.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseFastMover {
    pub product_key: Option<String>,
    pub name: String,
    pub sold_today_units: i64,
    pub selling_price_cents: i64,
    /// The product's own figures; absent when the product no longer exists.
    pub category: Option<String>,
    pub remaining_stock: Option<i64>,
    pub min_threshold: Option<i64>,
}
