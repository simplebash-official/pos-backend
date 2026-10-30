use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What a text element prints. `Code` is the value the barcode/QR encodes: the
/// product's barcode on a product label, the serial number on a serial label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum TextSource {
    ProductName,
    Price,
    Sku,
    Code,
    Category,
    ShopName,
    CustomText,
}
