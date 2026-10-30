use serde::Deserialize;
use utoipa::ToSchema;

/// One product on a receipt being recorded.
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptLineRequest {
    /// Foreign key of the product.
    pub product_key: String,
    /// Units received.
    pub quantity: i64,
    /// What the whole line costs in cents: the supplier's lot price, or quantity x price each.
    /// The unit cost is worked out from this (plus the line's share of extra charges), never sent.
    pub line_total_cents: i64,
    /// Price each received unit sells at, in cents.
    pub selling_price_cents: i64,
    /// One serial number per unit, required (and counted against `quantity`) when the product
    /// is serialized; omitted otherwise.
    #[serde(default)]
    pub serial_numbers: Option<Vec<String>>,
}
