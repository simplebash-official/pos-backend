use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One thing to print. A product label prints the product's barcode; a serial
/// label prints that unit's serial number at its own batch's price.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LabelItem {
    #[serde(rename_all = "camelCase")]
    Product {
        product_key: String,
        copies: u32,
        /// One of the product's current selling prices; required when the
        /// chosen template has a price element, `null` otherwise.
        price_cents: Option<i64>,
    },
    #[serde(rename_all = "camelCase")]
    Serial { serial_number: String, copies: u32 },
}
