use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One priced line of a quotation, embedded in the stored document and the
/// returned `Quotation`. A catalog line's `name`/`sku`/`unit_price_cents` are
/// resolved from the product at creation time and then frozen — a quotation
/// keeps reading the way it did when it was given to the customer.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuotationItem {
    /// Catalog product this line was priced from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_key: Option<String>,
    /// Item description shown on the printed quotation.
    pub name: String,
    /// Stock Keeping Unit, if the line came from the catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sku: Option<String>,
    /// Price per unit in cents.
    pub unit_price_cents: i64,
    /// Number of units offered.
    pub quantity: i64,
    /// Discount on this line in cents.
    pub discount_cents: i64,
    /// `unit_price_cents * quantity - discount_cents`, floored at 0.
    pub total_cents: i64,
}
