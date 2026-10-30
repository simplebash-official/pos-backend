use serde::Serialize;
use utoipa::ToSchema;

/// An item in a quotation that is tracked for stock fulfillment.
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PulseQuotationItem {
    pub product_key: Option<String>,
    pub name: String,
    pub sku: Option<String>,
    pub quoted_quantity: i64,
    pub current_stock_quantity: i64,
    pub unit_price_cents: i64,
    pub has_sufficient_stock: bool,
}
