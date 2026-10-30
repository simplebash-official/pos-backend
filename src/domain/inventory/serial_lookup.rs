use super::product::Product;
use super::product_serial::ProductSerial;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What scanning a serial number resolves to: the unit, its product and the
/// price it sells at (its own batch's selling price). Admin-only cost fields are
/// stripped from `product` by the route, like every other product read.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SerialLookup {
    pub serial: ProductSerial,
    pub product: Product,
    pub selling_price_cents: i64,
}
